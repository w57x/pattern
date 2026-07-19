use crate::server::{ClientState, Composer, GlobalState, SelectionSource};
use std::os::fd::AsFd;
use wayland_protocols::wp::primary_selection::zv1::server::{
    zwp_primary_selection_device_manager_v1::{self, ZwpPrimarySelectionDeviceManagerV1},
    zwp_primary_selection_device_v1::{self, ZwpPrimarySelectionDeviceV1},
    zwp_primary_selection_offer_v1::{self, ZwpPrimarySelectionOfferV1},
    zwp_primary_selection_source_v1::{self, ZwpPrimarySelectionSourceV1},
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<ZwpPrimarySelectionDeviceManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpPrimarySelectionDeviceManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        client: &wayland_server::Client,
        _resource: &ZwpPrimarySelectionDeviceManagerV1,
        request: zwp_primary_selection_device_manager_v1::Request,
        dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwp_primary_selection_device_manager_v1::Request::CreateSource { id } => {
                let source = data_init.init(id, ClientState);
                state
                    .primary_selection_sources
                    .insert(source.id(), (source.clone(), Vec::new()));
            }
            zwp_primary_selection_device_manager_v1::Request::GetDevice { id, .. } => {
                let device = data_init.init(id, ClientState);
                state.primary_selection_devices.push(device.clone());

                if let Some(focused_surface) = &state.input_focus
                    && let Some(focused_client) = focused_surface.client()
                    && focused_client.id() == client.id()
                {
                    if let Some(source) = &state.primary_selection {
                        let offer = client
                            .create_resource::<ZwpPrimarySelectionOfferV1, ClientState, Composer>(
                                dhandle,
                                device.version(),
                                ClientState,
                            )
                            .expect("Failed to create ZwpPrimarySelectionOfferV1");
                        device.data_offer(&offer);

                        let mime_types = match source {
                            SelectionSource::Standard(_) => {
                                state.data_sources.get(&source.id()).map(|(_, m)| m)
                            }
                            SelectionSource::Primary(_) => state
                                .primary_selection_sources
                                .get(&source.id())
                                .map(|(_, m)| m),
                            SelectionSource::DataControl(_) => {
                                state.data_control_sources.get(&source.id()).map(|(_, m)| m)
                            }
                        };

                        if let Some(mime_types) = mime_types {
                            for mime in mime_types {
                                offer.offer(mime.clone());
                            }
                        }
                        device.selection(Some(&offer));
                    } else {
                        device.selection(None);
                    }
                }
            }
            zwp_primary_selection_device_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwpPrimarySelectionDeviceV1,
        request: zwp_primary_selection_device_v1::Request,
        dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwp_primary_selection_device_v1::Request::SetSelection { source, .. } => {
                if state.primary_selection.as_ref().map(|s| s.id())
                    == source.as_ref().map(|s| s.id())
                {
                    return;
                }

                if let Some(old_source) = state.primary_selection.take() {
                    old_source.cancelled();
                }

                if let Some(new_source) = source {
                    state.primary_selection = Some(SelectionSource::Primary(new_source.clone()));
                    state.broadcast_primary_selection_offer(dhandle);
                } else {
                    state.primary_selection = None;
                    if let Some(focused_surface) = &state.input_focus
                        && let Some(focused_client) = focused_surface.client()
                    {
                        state.clear_primary_selection(&focused_client);
                    }
                }
            }
            zwp_primary_selection_device_v1::Request::Destroy => {
                state
                    .primary_selection_devices
                    .retain(|d| d.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionSourceV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwpPrimarySelectionSourceV1,
        request: zwp_primary_selection_source_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwp_primary_selection_source_v1::Request::Offer { mime_type } => {
                if let Some((_, mime_types)) =
                    state.primary_selection_sources.get_mut(&resource.id())
                {
                    mime_types.push(mime_type);
                }
            }
            zwp_primary_selection_source_v1::Request::Destroy => {
                if state.primary_selection.as_ref().map(|s| s.id()) == Some(resource.id()) {
                    state.primary_selection = None;
                }
                state.primary_selection_sources.remove(&resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionOfferV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwpPrimarySelectionOfferV1,
        request: zwp_primary_selection_offer_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwp_primary_selection_offer_v1::Request::Receive { mime_type, fd } => {
                if let Some(source) = &state.primary_selection {
                    source.send(mime_type, fd.as_fd());
                }
            }
            zwp_primary_selection_offer_v1::Request::Destroy => {}
            _ => {}
        }
    }
}
