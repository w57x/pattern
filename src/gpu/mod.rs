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

        // let mut options = std::fs::OpenOptions::new();
        // options.read(true);
        // options.write(true);
        // Card(options.open(path).unwrap())
        Card(gpu_file, drm_device)
    }

    pub fn find_primary_gpu() -> Option<std::path::PathBuf> {
        let mut enumerator = Enumerator::new().ok()?;

        // Filter for the DRM subsystem
        enumerator.match_subsystem("drm").ok()?;
        enumerator.match_sysname("card*").ok()?;

        for device in enumerator.scan_devices().ok()? {
            // You can get even more specific here, like checking
            // if it's the "boot_vga" device (the main GPU used by BIOS)
            return device.devnode().map(|p| p.to_path_buf());
        }

        None
    }

    pub fn fetch_card_info(&self) -> CardInfo {
        use drm::control::{self, Device};
        let resources = self
            .resource_handles()
            .expect("Failed to get DRM resource handles");

        let mut resource: Option<CardInfo> = None;

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
                        if let Ok(prop_info) = self.get_property(prop_handle) {
                            if prop_info.name().to_str() == Ok("EDID") {
                                if let control::property::Value::Blob(blob_id) =
                                    prop_info.value_type().convert_value(prop_val)
                                {
                                    if blob_id > 0 {
                                        if let Ok(blob) = self.get_property_blob(blob_id) {
                                            match Info::parse_edid(&blob) {
                                                Ok(info) => {
                                                    let make = info
                                                        .make()
                                                        .unwrap_or("Unknown".to_string());
                                                    let model = info
                                                        .model()
                                                        .unwrap_or("Monitor".to_string());

                                                    output_description = format!(
                                                        "{} {} ({})",
                                                        make, model, output_name
                                                    );
                                                }
                                                Err(e) => {
                                                    warn!(
                                                        "Failed to parse EDID for {}: {}",
                                                        output_name, e
                                                    );
                                                    output_description = format!(
                                                        "Generic Monitor ({})",
                                                        output_name
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Pick a mode (the resolution)
                // Usually, the first mode is the "preferred" native resolution
                if let Some(mode) = connector.modes().get(0) {
                    info!("Selected mode: {:?}", mode.name());

                    // Match this connector to a CRTC (The display controller)
                    let crtc_handle = find_crtc(self, &resources, &connector);
                    info!("Linked to CRTC: {:?}", crtc_handle);

                    resource = Some(CardInfo {
                        mode: mode.clone(),
                        crtc_handle,
                        connector_handle,
                        name: output_name,
                        description: output_description,
                    });

                    break;
                }
            }
        }

        if resource.is_none() {
            panic!("Cannot fetch gpu display and connection info");
        }

        return resource.unwrap();
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

#[derive(Clone)]
#[allow(unused)]
pub struct CardInfo {
    pub mode: drm::control::Mode,
    pub crtc_handle: drm::control::crtc::Handle,
    pub connector_handle: drm::control::connector::Handle,
    pub name: String,
    pub description: String,
}

fn find_crtc(
    card: &Card,
    res: &drm::control::ResourceHandles,
    conn: &drm::control::connector::Info,
) -> drm::control::crtc::Handle {
    use drm::control::Device;

    // Check if the connector is already mapped to an active encoder/crtc
    if let Some(encoder_handle) = conn.current_encoder() {
        let encoder = card
            .get_encoder(encoder_handle)
            .expect("Failed to get encoder info");
        if let Some(crtc) = encoder.crtc() {
            return crtc;
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

        // If the list isn't empty, we've found our match!
        if let Some(&first_matched_crtc) = matching_crtcs.first() {
            return first_matched_crtc;
        }
    }

    panic!("Could not find a compatible CRTC for the connector");
}
