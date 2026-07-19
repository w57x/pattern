use wayland_protocols::ext::session_lock::v1::server::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ext_session_lock_v1::ExtSessionLockV1,
};
use wayland_server::{
    Dispatch, GlobalDispatch, Resource, backend::ObjectId, protocol::wl_surface::WlSurface,
};

use crate::server::{ClientState, Composer, GlobalState};

pub struct SessionLockState {
    pub lock: ExtSessionLockV1,
    pub surfaces: Vec<(ExtSessionLockSurfaceV1, WlSurface, ObjectId)>,
}

impl GlobalDispatch<ExtSessionLockManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ExtSessionLockManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ExtSessionLockManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ExtSessionLockManagerV1,
        request: <ExtSessionLockManagerV1 as wayland_server::Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        use wayland_protocols::ext::session_lock::v1::server::ext_session_lock_manager_v1::Request;
        match request {
            Request::Destroy => {}
            Request::Lock { id } => {
                let lock = data_init.init(id, ClientState);
                if state.session_lock.is_some() {
                    // Protocol requires sending finished if a lock is already active
                    lock.finished();
                    return;
                }
                lock.locked();
                state.session_lock = Some(SessionLockState {
                    lock,
                    surfaces: Vec::new(),
                });
                state.needs_redraw = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtSessionLockV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ExtSessionLockV1,
        request: <ExtSessionLockV1 as wayland_server::Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        use wayland_protocols::ext::session_lock::v1::server::ext_session_lock_v1::Request;
        match request {
            Request::GetLockSurface {
                id,
                surface,
                output,
            } => {
                let lock_surface = data_init.init(id, ClientState);

                if let Some(lock_state) = &mut state.session_lock {
                    if lock_state.lock.id() == resource.id() {
                        lock_state.surfaces.push((
                            lock_surface.clone(),
                            surface.clone(),
                            output.id(),
                        ));

                        // Send initial configure
                        let mut sent = false;
                        if let Some(wl_out) = state.outputs.iter().find(|o| o.id() == output.id()) {
                            if let Some(output_idx) = wl_out.data::<usize>() {
                                if let Some(out_info) = state.outputs_info.get(*output_idx) {
                                    state.serial += 1;
                                    lock_surface.configure(
                                        state.serial,
                                        out_info.width as u32,
                                        out_info.height as u32,
                                    );
                                    sent = true;
                                }
                            }
                        }

                        if !sent {
                            tracing::error!(
                                "Failed to send configure: could not find output_idx for WlOutput {:?}",
                                output.id()
                            );
                        }

                        state.set_input_focus(Some(surface.clone()), _dhandle);
                    }
                }
            }
            Request::UnlockAndDestroy => {
                if let Some(lock_state) = &state.session_lock {
                    if lock_state.lock.id() == resource.id() {
                        state.session_lock = None;
                        state.needs_redraw = true;
                    }
                }
            }
            Request::Destroy => {
                // Sent after finished
                if let Some(lock_state) = &state.session_lock {
                    if lock_state.lock.id() == resource.id() {
                        state.session_lock = None;
                        state.needs_redraw = true;
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtSessionLockSurfaceV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ExtSessionLockSurfaceV1,
        request: <ExtSessionLockSurfaceV1 as wayland_server::Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        use wayland_protocols::ext::session_lock::v1::server::ext_session_lock_surface_v1::Request;
        match request {
            Request::Destroy => {}
            Request::AckConfigure { serial: _ } => {
                // Client acknowledged our configure
            }
            _ => {}
        }
    }
}
