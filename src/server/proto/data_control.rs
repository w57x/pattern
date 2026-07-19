use crate::server::{ClientState, Composer, GlobalState, SelectionSource};
use std::os::unix::io::AsFd;
use wayland_protocols_wlr::data_control::v1::server::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::{self, ZwlrDataControlManagerV1},
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<ZwlrDataControlManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwlrDataControlManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZwlrDataControlManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        client: &wayland_server::Client,
        _resource: &ZwlrDataControlManagerV1,
        request: <ZwlrDataControlManagerV1 as Resource>::Request,
        dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwlr_data_control_manager_v1::Request::CreateDataSource { id } => {
                let source = data_init.init(id, ClientState);
                state
                    .data_control_sources
                    .insert(source.id(), (source.clone(), Vec::new()));
            }
            zwlr_data_control_manager_v1::Request::GetDataDevice { id, seat: _ } => {
                let device = data_init.init(id, ClientState);
                state.data_control_devices.push(device.clone());

                // Immediately send current selection
                if let Some(source) = &state.selection {
                    let offer = client
                        .create_resource::<ZwlrDataControlOfferV1, ClientState, Composer>(
                            dhandle,
                            device.version(),
                            ClientState,
                        )
                        .expect("Failed to create ZwlrDataControlOfferV1");
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

                // Immediately send current primary selection
                if let Some(source) = &state.primary_selection {
                    let offer = client
                        .create_resource::<ZwlrDataControlOfferV1, ClientState, Composer>(
                            dhandle,
                            device.version(),
                            ClientState,
                        )
                        .expect("Failed to create ZwlrDataControlOfferV1");
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
                    device.primary_selection(Some(&offer));
                } else {
                    device.primary_selection(None);
                }
            }
            zwlr_data_control_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwlrDataControlDeviceV1,
        request: <ZwlrDataControlDeviceV1 as Resource>::Request,
        dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwlr_data_control_device_v1::Request::SetSelection { source } => {
                if state.selection.as_ref().map(|s| s.id()) == source.as_ref().map(|s| s.id()) {
                    return;
                }

                if let Some(old_source) = state.selection.take() {
                    old_source.cancelled();
                }

                if let Some(new_source) = source {
                    state.selection = Some(SelectionSource::DataControl(new_source.clone()));
                    state.broadcast_selection_offer(dhandle);
                } else {
                    state.selection = None;
                    if let Some(focus) = &state.input_focus
                        && let Some(client) = focus.client()
                    {
                        state.clear_selection(&client);
                    }
                }
            }
            zwlr_data_control_device_v1::Request::SetPrimarySelection { source } => {
                if state.primary_selection.as_ref().map(|s| s.id())
                    == source.as_ref().map(|s| s.id())
                {
                    return;
                }

                if let Some(old_source) = state.primary_selection.take() {
                    old_source.cancelled();
                }

                if let Some(new_source) = source {
                    state.primary_selection =
                        Some(SelectionSource::DataControl(new_source.clone()));
                    state.broadcast_primary_selection_offer(dhandle);
                } else {
                    state.primary_selection = None;
                    if let Some(focus) = &state.input_focus
                        && let Some(client) = focus.client()
                    {
                        state.clear_primary_selection(&client);
                    }
                }
            }
            zwlr_data_control_device_v1::Request::Destroy => {
                state
                    .data_control_devices
                    .retain(|d| d.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwlrDataControlSourceV1,
        request: <ZwlrDataControlSourceV1 as Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwlr_data_control_source_v1::Request::Offer { mime_type } => {
                if let Some((_, mime_types)) = state.data_control_sources.get_mut(&resource.id()) {
                    mime_types.push(mime_type);
                }
            }
            zwlr_data_control_source_v1::Request::Destroy => {
                if state.selection.as_ref().map(|s| s.id()) == Some(resource.id()) {
                    state.selection = None;
                }
                state.data_control_sources.remove(&resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrDataControlOfferV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwlrDataControlOfferV1,
        request: <ZwlrDataControlOfferV1 as Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwlr_data_control_offer_v1::Request::Receive { mime_type, fd } => {
                // Connect offer to the actual data_source
                if let Some(source) = &state.selection {
                    source.send(mime_type, fd.as_fd());
                }
            }
            zwlr_data_control_offer_v1::Request::Destroy => {}
            _ => {}
        }
    }
}
