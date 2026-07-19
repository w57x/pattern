use std::sync::{Arc, Mutex};

use crate::server::{ClientState, Composer, GlobalState, ShmBuffer};
use wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm::WlShm, wl_shm_pool::WlShmPool};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlShm, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlShm>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        let shm = data_init.init(resource, ClientState);

        shm.format(wayland_server::protocol::wl_shm::Format::Xrgb8888);
        shm.format(wayland_server::protocol::wl_shm::Format::Argb8888);
    }
}

impl Dispatch<WlShm, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &WlShm,
        request: wayland_server::protocol::wl_shm::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let wayland_server::protocol::wl_shm::Request::CreatePool { id, size, fd, .. } = request
        {
            let mmap = Arc::new(Mutex::new(unsafe {
                memmap2::MmapOptions::new()
                    .len(size as usize)
                    .map_mut(&fd)
                    .unwrap()
            }));
            let pool = data_init.init(id, ClientState);
            state.pools.insert(pool.id(), (fd, mmap));
        }
    }
}

impl Dispatch<WlShmPool, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &WlShmPool,
        request: wayland_server::protocol::wl_shm_pool::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        use std::os::fd::AsFd;

        match request {
            wayland_server::protocol::wl_shm_pool::Request::CreateBuffer {
                id,
                offset,
                width,
                height,
                stride,
                ..
            } => {
                let buffer = data_init.init(id, ClientState);
                if let Some((_, mmap)) = state.pools.get(&resource.id()) {
                    state.buffers.insert(
                        buffer.id(),
                        ShmBuffer {
                            pool_id: resource.id(),
                            offset,
                            width,
                            height,
                            stride,
                            mmap: mmap.clone(),
                        },
                    );
                }
            }

            wayland_server::protocol::wl_shm_pool::Request::Resize { size } => {
                if let Some((fd, mmap)) = state.pools.get_mut(&resource.id()) {
                    let mut lock = mmap.lock().unwrap();
                    *lock = unsafe {
                        memmap2::MmapOptions::new()
                            .len(size as usize)
                            .map_mut(&fd.as_fd())
                            .unwrap()
                    };
                }
            }

            wayland_server::protocol::wl_shm_pool::Request::Destroy => {
                state.pools.remove(&resource.id());
            }

            _ => {}
        }
    }
}

impl Dispatch<WlBuffer, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &WlBuffer,
        request: wayland_server::protocol::wl_buffer::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let wayland_server::protocol::wl_buffer::Request::Destroy = request {
            state.buffers.remove(&resource.id());
            state.dmabuffers.remove(&resource.id());
            state.buffer_textures.remove(&resource.id());
        }
    }
}
