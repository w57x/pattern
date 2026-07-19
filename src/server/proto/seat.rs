use crate::server::{Composer, SelectionSource};
use std::os::fd::AsFd;
use wayland_server::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::WlDataDeviceManager,
    wl_data_offer::WlDataOffer, wl_data_source::WlDataSource, wl_keyboard::WlKeyboard,
    wl_pointer::WlPointer, wl_seat::WlSeat,
};
use wayland_server::protocol::{wl_keyboard, wl_seat};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlSeat, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlSeat>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let seat = data_init.init(resource, ());

        seat.capabilities(wl_seat::Capability::Pointer | wl_seat::Capability::Keyboard);
        seat.name("pattern-seat".to_string());
    }
}

impl Dispatch<WlSeat, ()> for Composer {
    fn request(
        state: &mut Self,
        client: &wayland_server::Client,
        _resource: &WlSeat,
        request: wayland_server::protocol::wl_seat::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wl_seat::Request::GetPointer { id } => {
                let pointer = data_init.init(id, ());
                state.pointers.push(pointer);
            }
            wl_seat::Request::GetKeyboard { id } => {
                let keyboard = data_init.init(id, ());
                let fd = state.keymap_fd.as_fd();
                keyboard.keymap(wl_keyboard::KeymapFormat::XkbV1, fd, state.keymap_size);

                if keyboard.version() >= 4 {
                    let (rate, delay) = {
                        let cfg = state.config_manager.config.lock().unwrap();
                        (cfg.input.repeat_rate as i32, cfg.input.repeat_delay as i32)
                    };
                    keyboard.repeat_info(rate, delay);
                }

                if let Some(focused_surface) = &state.input_focus
                    && let Some(focused_client) = focused_surface.client()
                    && focused_client.id() == client.id()
                {
                    state.serial += 1;
                    keyboard.enter(state.serial, focused_surface, Vec::new());
                    keyboard.modifiers(state.serial, 0, 0, 0, 0);
                }

                state.keyboards.push(keyboard);
            }
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlPointer,
        request: wayland_server::protocol::wl_pointer::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        use wayland_server::protocol::wl_pointer::Request;
        match request {
            Request::SetCursor {
                surface,
                hotspot_x,
                hotspot_y,
                ..
            } => {
                state.cursor_shape = None;
                if let Some(surf) = surface {
                    state.cursor_surface = Some((surf, hotspot_x, hotspot_y));
                } else {
                    state.cursor_surface = None;
                }
            }
            Request::Release => {
                state.pointers.retain(|p| p.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlKeyboard, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlKeyboard,
        request: wayland_server::protocol::wl_keyboard::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let wayland_server::protocol::wl_keyboard::Request::Release = request {
            state.keyboards.retain(|k| k.id() != resource.id());
        }
    }
}

impl GlobalDispatch<WlDataDeviceManager, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlDataDeviceManager>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlDataDeviceManager, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlDataDeviceManager,
        request: wayland_server::protocol::wl_data_device_manager::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_data_device_manager::Request::GetDataDevice {
                id, ..
            } => {
                let device = data_init.init(id, ());
                state.data_devices.push(device.clone());

                if let Some(focused_surface) = &state.input_focus
                    && let Some(focused_client) = focused_surface.client()
                    && focused_client.id() == _client.id()
                {
                    if let Some(source) = &state.selection {
                        let offer = _client
                            .create_resource::<WlDataOffer, (), Self>(
                                _dhandle,
                                device.version(),
                                (),
                            )
                            .expect("Failed to create WlDataOffer");
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
            wayland_server::protocol::wl_data_device_manager::Request::CreateDataSource { id } => {
                let source = data_init.init(id, ());
                state
                    .data_sources
                    .insert(source.id(), (source.clone(), Vec::new()));
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlDataDevice,
        request: wayland_server::protocol::wl_data_device::Request,
        _data: &(),
        dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_data_device::Request::SetSelection { source, .. } => {
                if state.selection.as_ref().map(|s| s.id()) == source.as_ref().map(|s| s.id()) {
                    return;
                }

                if let Some(old_source) = state.selection.take() {
                    old_source.cancelled();
                }

                if let Some(new_source) = source {
                    state.selection = Some(SelectionSource::Standard(new_source.clone()));
                    state.broadcast_selection_offer(dhandle);
                } else {
                    state.selection = None;
                    if let Some(focused_surface) = &state.input_focus
                        && let Some(focused_client) = focused_surface.client()
                    {
                        state.clear_selection(&focused_client);
                    }
                }
            }
            wayland_server::protocol::wl_data_device::Request::Release => {
                state.data_devices.retain(|d| d.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlDataSource,
        request: wayland_server::protocol::wl_data_source::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_data_source::Request::Offer { mime_type } => {
                if let Some((_, mime_types)) = state.data_sources.get_mut(&resource.id()) {
                    mime_types.push(mime_type);
                }
            }
            wayland_server::protocol::wl_data_source::Request::Destroy => {
                if state.selection.as_ref().map(|s| s.id()) == Some(resource.id()) {
                    state.selection = None;
                }
                state.data_sources.remove(&resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataOffer, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlDataOffer,
        request: wayland_server::protocol::wl_data_offer::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_data_offer::Request::Accept { mime_type, .. } => {
                if let Some(source) = &state.selection {
                    source.target(mime_type);
                }
            }
            wayland_server::protocol::wl_data_offer::Request::Receive { mime_type, fd } => {
                if let Some(source) = &state.selection {
                    source.send(mime_type, fd.as_fd());
                }
            }
            wayland_server::protocol::wl_data_offer::Request::Destroy => {}
            _ => {}
        }
    }
}
