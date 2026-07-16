use libdisplay_info::info::Info;
use std::{
    cell::RefCell,
    os::fd::{AsFd, BorrowedFd},
    rc::Rc,
};
use tracing::{info, warn};
use udev::Enumerator;

pub mod buffer;

/// A simple wrapper for a device node.
pub struct Card(std::fs::File, libseat::Device);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Card {
    /// Simple helper methods for opening a `Card`.
    pub fn open(p: Option<&str>, seat: Rc<RefCell<libseat::Seat>>) -> Self {
        let path = if p.is_some() {
            std::path::PathBuf::from(p.unwrap())
        } else {
            let chosen = Card::find_primary_gpu();
            if chosen.is_some() {
                chosen.unwrap()
            } else {
                panic!("Unable to locate a GPU to use");
            }
        };

        let drm_device = seat
            .borrow_mut()
            .open_device(&path)
            .expect("Seat manager refused to open the GPU");

        let drm_fd = nix::unistd::dup(&drm_device).unwrap();
        let gpu_file = std::fs::File::from(drm_fd);
        let card = Card(gpu_file, drm_device);
        use drm::{ClientCapability, Device as _};
        let _ = card.set_client_capability(ClientCapability::UniversalPlanes, true);
        let _ = card.set_client_capability(ClientCapability::Atomic, true);
        card
    }

    pub fn find_primary_gpu() -> Option<std::path::PathBuf> {
        let mut enumerator = Enumerator::new().ok()?;

        // Filter for the DRM subsystem
        enumerator.match_subsystem("drm").ok()?;
        enumerator.match_sysname("card*").ok()?;

        enumerator
            .scan_devices()
            .ok()?
            .next()
            .and_then(|device| device.devnode().map(|p| p.to_path_buf()))
    }

    pub fn find_property(
        &self,
        handle: impl drm::control::ResourceHandle,
        name: &str,
    ) -> Option<drm::control::property::Handle> {
        use drm::control::Device;
        if let Ok(props) = self.get_properties(handle) {
            let (handles, _) = props.as_props_and_values();
            for &prop_handle in handles {
                if let Ok(info) = self.get_property(prop_handle)
                    && info.name().to_str() == Ok(name)
                {
                    return Some(prop_handle);
                }
            }
        }
        None
    }

    pub fn find_primary_plane(
        &self,
        crtc_handle: drm::control::crtc::Handle,
        assigned_planes: &[drm::control::plane::Handle],
    ) -> Option<drm::control::plane::Handle> {
        use drm::control::Device;
        let resources = self.resource_handles().ok()?;
        let planes = self.plane_handles().ok()?;

        for plane_handle in planes {
            if assigned_planes.contains(&plane_handle) {
                continue;
            }
            let plane_info = self.get_plane(plane_handle).ok()?;

            let compatible_crtcs = resources.filter_crtcs(plane_info.possible_crtcs());
            if !compatible_crtcs.contains(&crtc_handle) {
                continue;
            }

            if let Ok(props) = self.get_properties(plane_handle) {
                let (prop_handles, prop_values) = props.as_props_and_values();
                for (&prop_handle, &prop_value) in prop_handles.iter().zip(prop_values.iter()) {
                    if let Ok(prop_info) = self.get_property(prop_handle)
                        && prop_info.name().to_str() == Ok("type")
                        && prop_value == (drm::control::PlaneType::Primary as u32).into()
                    {
                        return Some(plane_handle);
                    }
                }
            }
        }
        None
    }

    pub fn find_cursor_plane(
        &self,
        crtc_handle: drm::control::crtc::Handle,
        assigned_planes: &[drm::control::plane::Handle],
    ) -> Option<drm::control::plane::Handle> {
        use drm::control::Device;
        let resources = self.resource_handles().ok()?;
        let planes = self.plane_handles().ok()?;

        for plane_handle in planes {
            if assigned_planes.contains(&plane_handle) {
                continue;
            }
            let plane_info = self.get_plane(plane_handle).ok()?;

            let compatible_crtcs = resources.filter_crtcs(plane_info.possible_crtcs());
            if !compatible_crtcs.contains(&crtc_handle) {
                continue;
            }

            if let Ok(props) = self.get_properties(plane_handle) {
                let (prop_handles, prop_values) = props.as_props_and_values();
                for (&prop_handle, &prop_value) in prop_handles.iter().zip(prop_values.iter()) {
                    if let Ok(prop_info) = self.get_property(prop_handle)
                        && prop_info.name().to_str() == Ok("type")
                        && prop_value == (drm::control::PlaneType::Cursor as u32).into()
                    {
                        return Some(plane_handle);
                    }
                }
            }
        }
        None
    }

    pub fn fetch_card_info(&self) -> CardInfo {
        let infos = self.fetch_card_infos();
        infos[0].card_info.clone()
    }

    pub fn fetch_card_infos(&self) -> Vec<OutputLayoutInfo> {
        use drm::control::{self, Device};
        let resources = self
            .resource_handles()
            .expect("Failed to get DRM resource handles");

        let mut outputs = Vec::new();
        let mut current_x = 0;

        let mut assigned_crtcs = Vec::new();
        let mut assigned_planes = Vec::new();

        for &connector_handle in resources.connectors() {
            let connector = self
                .get_connector(connector_handle, false)
                .expect("Failed to get connector info");

            if connector.state() == control::connector::State::Connected {
                info!("Found connected connector: {:?}", connector.interface());

                let interface_name = match connector.interface() {
                    control::connector::Interface::EmbeddedDisplayPort => "eDP",
                    control::connector::Interface::DisplayPort => "DP",
                    control::connector::Interface::HDMIA => "HDMI-A",
                    control::connector::Interface::HDMIB => "HDMI-B",
                    control::connector::Interface::VGA => "VGA",
                    control::connector::Interface::DVID => "DVI-D",
                    _ => "Unknown",
                };
                let output_name = format!("{}-{}", interface_name, connector.interface_id());

                let mut output_description = format!("Generic Monitor ({})", output_name);

                if let Ok(props) = self.get_properties(connector_handle) {
                    let (prop_handles, prop_values) = props.as_props_and_values();

                    for (&prop_handle, &prop_val) in prop_handles.iter().zip(prop_values.iter()) {
                        if let Ok(prop_info) = self.get_property(prop_handle)
                            && prop_info.name().to_str() == Ok("EDID")
                            && let control::property::Value::Blob(blob_id) =
                                prop_info.value_type().convert_value(prop_val)
                            && blob_id > 0
                            && let Ok(blob) = self.get_property_blob(blob_id)
                        {
                            match Info::parse_edid(&blob) {
                                Ok(info) => {
                                    let make = info.make().unwrap_or("Unknown".to_string());
                                    let model = info.model().unwrap_or("Monitor".to_string());

                                    output_description =
                                        format!("{} {} ({})", make, model, output_name);
                                }
                                Err(e) => {
                                    warn!("Failed to parse EDID for {}: {}", output_name, e);
                                    output_description =
                                        format!("Generic Monitor ({})", output_name);
                                }
                            }
                        }
                    }
                }

                // Pick a mode (the resolution)
                // Usually, the first mode is the "preferred" native resolution
                if let Some(mode) = connector.modes().first() {
                    info!("Selected mode: {:?}", mode.name());

                    if let Some(crtc_handle) =
                        find_crtc(self, &resources, &connector, &assigned_crtcs)
                    {
                        info!("Linked to CRTC: {:?}", crtc_handle);

                        let crtc_info =
                            self.get_crtc(crtc_handle).expect("Failed to get CRTC info");
                        let gamma_size = crtc_info.gamma_length();

                        if let Some(primary_plane) =
                            self.find_primary_plane(crtc_handle, &assigned_planes)
                        {
                            let crtc_active_prop = self
                                .find_property(crtc_handle, "ACTIVE")
                                .expect("Failed to find CRTC ACTIVE property");
                            let crtc_mode_id_prop = self
                                .find_property(crtc_handle, "MODE_ID")
                                .expect("Failed to find CRTC MODE_ID property");
                            let crtc_gamma_lut_prop = self
                                .find_property(crtc_handle, "GAMMA_LUT")
                                .expect("Failed to find CRTC GAMMA_LUT property");
                            let plane_crtc_id_prop = self
                                .find_property(primary_plane, "CRTC_ID")
                                .expect("Failed to find Plane CRTC_ID property");
                            let plane_fb_id_prop = self
                                .find_property(primary_plane, "FB_ID")
                                .expect("Failed to find Plane FB_ID property");
                            let conn_crtc_id_prop = self
                                .find_property(connector_handle, "CRTC_ID")
                                .expect("Failed to find Connector CRTC_ID property");

                            let src_x_prop = self.find_property(primary_plane, "SRC_X");
                            let src_y_prop = self.find_property(primary_plane, "SRC_Y");
                            let src_w_prop = self.find_property(primary_plane, "SRC_W");
                            let src_h_prop = self.find_property(primary_plane, "SRC_H");
                            let crtc_x_prop = self.find_property(primary_plane, "CRTC_X");
                            let crtc_y_prop = self.find_property(primary_plane, "CRTC_Y");
                            let crtc_w_prop = self.find_property(primary_plane, "CRTC_W");
                            let crtc_h_prop = self.find_property(primary_plane, "CRTC_H");

                            let cursor_plane =
                                self.find_cursor_plane(crtc_handle, &assigned_planes);
                            let cursor_crtc_id_prop =
                                cursor_plane.and_then(|p| self.find_property(p, "CRTC_ID"));
                            let cursor_fb_id_prop =
                                cursor_plane.and_then(|p| self.find_property(p, "FB_ID"));

                            assigned_crtcs.push(crtc_handle);
                            assigned_planes.push(primary_plane);
                            if let Some(cp) = cursor_plane {
                                assigned_planes.push(cp);
                            }

                            let (w, h) = mode.size();
                            let card_info = CardInfo {
                                mode: *mode,
                                crtc_handle,
                                connector_handle,
                                name: output_name,
                                description: output_description,
                                gamma_size,
                                primary_plane,
                                crtc_active_prop,
                                crtc_mode_id_prop,
                                crtc_gamma_lut_prop,
                                plane_crtc_id_prop,
                                plane_fb_id_prop,
                                conn_crtc_id_prop,
                                cursor_plane,
                                cursor_crtc_id_prop,
                                cursor_fb_id_prop,
                                src_x_prop,
                                src_y_prop,
                                src_w_prop,
                                src_h_prop,
                                crtc_x_prop,
                                crtc_y_prop,
                                crtc_w_prop,
                                crtc_h_prop,
                            };

                            outputs.push(OutputLayoutInfo {
                                card_info,
                                x: current_x,
                                y: 0,
                                width: w as i32,
                                height: h as i32,
                            });

                            current_x += w as i32;
                        }
                    }
                }
            }
        }

        if outputs.is_empty() {
            panic!("Cannot fetch gpu display and connection info");
        }

        outputs
    }

    pub fn get_driver(&self) -> std::io::Result<drm::Driver> {
        drm::Device::get_driver(self)
    }
}

impl drm::Device for Card {}
impl drm::control::Device for Card {}

impl std::fmt::Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Card(.file = {:?}, .seat = {:?})", self.0, self.1)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct DrmColorLut {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
    pub reserved: u16,
}

#[derive(Clone)]
#[allow(unused)]
pub struct CardInfo {
    pub mode: drm::control::Mode,
    pub crtc_handle: drm::control::crtc::Handle,
    pub connector_handle: drm::control::connector::Handle,
    pub name: String,
    pub description: String,
    pub gamma_size: u32,
    pub primary_plane: drm::control::plane::Handle,
    pub crtc_active_prop: drm::control::property::Handle,
    pub crtc_mode_id_prop: drm::control::property::Handle,
    pub crtc_gamma_lut_prop: drm::control::property::Handle,
    pub plane_crtc_id_prop: drm::control::property::Handle,
    pub plane_fb_id_prop: drm::control::property::Handle,
    pub conn_crtc_id_prop: drm::control::property::Handle,
    pub cursor_plane: Option<drm::control::plane::Handle>,
    pub cursor_crtc_id_prop: Option<drm::control::property::Handle>,
    pub cursor_fb_id_prop: Option<drm::control::property::Handle>,
    pub src_x_prop: Option<drm::control::property::Handle>,
    pub src_y_prop: Option<drm::control::property::Handle>,
    pub src_w_prop: Option<drm::control::property::Handle>,
    pub src_h_prop: Option<drm::control::property::Handle>,
    pub crtc_x_prop: Option<drm::control::property::Handle>,
    pub crtc_y_prop: Option<drm::control::property::Handle>,
    pub crtc_w_prop: Option<drm::control::property::Handle>,
    pub crtc_h_prop: Option<drm::control::property::Handle>,
}

#[derive(Clone)]
pub struct OutputLayoutInfo {
    pub card_info: CardInfo,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

fn find_crtc(
    card: &Card,
    res: &drm::control::ResourceHandles,
    conn: &drm::control::connector::Info,
    assigned_crtcs: &[drm::control::crtc::Handle],
) -> Option<drm::control::crtc::Handle> {
    use drm::control::Device;

    // Check if the connector is already mapped to an active encoder/crtc
    if let Some(encoder_handle) = conn.current_encoder() {
        let encoder = card
            .get_encoder(encoder_handle)
            .expect("Failed to get encoder info");
        if let Some(crtc) = encoder.crtc()
            && !assigned_crtcs.contains(&crtc)
        {
            return Some(crtc);
        }
    }

    // Iterate through all encoders supported by this specific physical connector
    for &encoder_handle in conn.encoders() {
        let encoder = card
            .get_encoder(encoder_handle)
            .expect("Failed to get encoder info");

        // Get the bitmask (filter) of possible CRTCs for this encoder
        let filter = encoder.possible_crtcs();

        // RESOLVE: Ask the ResourceHandles to give us the actual CRTC handles
        // that match this filter bitmask.
        let matching_crtcs = res.filter_crtcs(filter);

        for &matched_crtc in &matching_crtcs {
            if !assigned_crtcs.contains(&matched_crtc) {
                return Some(matched_crtc);
            }
        }
    }

    None
}
