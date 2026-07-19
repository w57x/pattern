use std::os::unix::io::OwnedFd;

use wayland_protocols::wp::linux_drm_syncobj::v1::server::{
    wp_linux_drm_syncobj_manager_v1::WpLinuxDrmSyncobjManagerV1,
    wp_linux_drm_syncobj_surface_v1::WpLinuxDrmSyncobjSurfaceV1,
    wp_linux_drm_syncobj_timeline_v1::WpLinuxDrmSyncobjTimelineV1,
};
use wayland_server::{Dispatch, DisplayHandle, GlobalDispatch, Resource, backend::ClientId};

use ash::vk;

use crate::server::{ClientState, Composer, GlobalState, SyncobjSurfaceData};

pub struct Timeline {
    pub fd: OwnedFd,
}

#[derive(Default, Clone)]
pub struct SurfaceSyncObjState {
    pub acquire_point: Option<(vk::Semaphore, u64)>, // New buffer ready
    pub current_release: Option<(vk::Semaphore, u64)>, // Buffer currently on screen
    pub signal_queue: Vec<(vk::Semaphore, u64)>,     // Old buffers to be released
}

impl GlobalDispatch<WpLinuxDrmSyncobjManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WpLinuxDrmSyncobjManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<WpLinuxDrmSyncobjManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &WpLinuxDrmSyncobjManagerV1,
        request: <WpLinuxDrmSyncobjManagerV1 as wayland_server::Resource>::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        use wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_manager_v1::Request;
        match request {
            Request::Destroy => {
                // do nothing
            }
            Request::GetSurface { id, surface } => {
                state.explicit_sync_surfaces.insert(surface.id());
                data_init.init(id, SyncobjSurfaceData(surface.id()));
            }
            Request::ImportTimeline { id, fd } => {
                data_init.init(id, Timeline { fd });
            }
            _ => {}
        }
    }
}

impl Dispatch<WpLinuxDrmSyncobjSurfaceV1, Composer> for SyncobjSurfaceData {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &WpLinuxDrmSyncobjSurfaceV1,
        request: <WpLinuxDrmSyncobjSurfaceV1 as wayland_server::Resource>::Request,
        dhandle: &DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        use wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_surface_v1::Request;
        let surface_id = &self.0;
        match request {
            Request::Destroy => {
                state.explicit_sync_surfaces.remove(surface_id);
                state.pending_syncobj_state.remove(surface_id);
                state.syncobj_state.remove(surface_id);
            }
            Request::SetAcquirePoint {
                timeline,
                point_hi,
                point_lo,
            } => {
                let point = ((point_hi as u64) << 32) | (point_lo as u64);
                if let Some(sem) = state.get_or_import_timeline_semaphore(&timeline.id(), dhandle) {
                    let sync_state = state
                        .pending_syncobj_state
                        .entry(surface_id.clone())
                        .or_default();
                    sync_state.acquire_point = Some((sem, point));
                }
            }
            Request::SetReleasePoint {
                timeline,
                point_hi,
                point_lo,
            } => {
                let point = ((point_hi as u64) << 32) | (point_lo as u64);
                if let Some(sem) = state.get_or_import_timeline_semaphore(&timeline.id(), dhandle) {
                    let sync_state = state
                        .pending_syncobj_state
                        .entry(surface_id.clone())
                        .or_default();

                    // If we already had a current release point, it's now "old" and can be signaled
                    if let Some(old_release) = sync_state.current_release.take() {
                        sync_state.signal_queue.push(old_release);
                    }
                    sync_state.current_release = Some((sem, point));
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WpLinuxDrmSyncobjTimelineV1, Composer> for Timeline {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &WpLinuxDrmSyncobjTimelineV1,
        request: <WpLinuxDrmSyncobjTimelineV1 as wayland_server::Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        use wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_timeline_v1::Request;
        if let Request::Destroy = request
            && let Some(e) = state.syncobj_timelines.remove(&resource.id())
        {
            state.dead_semaphores.push(e);
        }
    }

    fn destroyed(
        &self,
        state: &mut Composer,
        _client: ClientId,
        resource: &WpLinuxDrmSyncobjTimelineV1,
    ) {
        if let Some(e) = state.syncobj_timelines.remove(&resource.id()) {
            state.dead_semaphores.push(e);
        }
    }
}
