use crate::server::{ServerState, ShmBuffer};
use wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm::WlShm, wl_shm_pool::WlShmPool};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlShm, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlShm>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let shm = data_init.init(resource, ());

        shm.format(wayland_server::protocol::wl_shm::Format::Xrgb8888);
        shm.format(wayland_server::protocol::wl_shm::Format::Argb8888);
    }
}

impl Dispatch<WlShm, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlShm,
        request: wayland_server::protocol::wl_shm::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let wayland_server::protocol::wl_shm::Request::CreatePool { id, size, fd, .. } = request
        {
            let mmap = unsafe {
                memmap2::MmapOptions::new()
                    .len(size as usize)
                    .map_mut(&fd)
                    .unwrap()
            };
            let pool = data_init.init(id, ());
            state.pools.insert(pool.id(), (fd, mmap));
        }
    }
}

impl Dispatch<WlShmPool, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlShmPool,
        request: wayland_server::protocol::wl_shm_pool::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
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
                let buffer = data_init.init(id, ());
                state.buffers.insert(
                    buffer.id(),
                    ShmBuffer {
                        pool_id: resource.id(),
                        offset,
                        width,
                        height,
                        stride,
                    },
                );
            }

            wayland_server::protocol::wl_shm_pool::Request::Resize { size } => {
                println!("[pattern]: Client requested resize of SHM pool to {}", size);
                if let Some((fd, mmap)) = state.pools.get_mut(&resource.id()) {
                    *mmap = unsafe {
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

impl Dispatch<WlBuffer, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlBuffer,
        request: wayland_server::protocol::wl_buffer::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_buffer::Request::Destroy => {
                state.buffers.remove(&resource.id());
                state.dmabuffers.remove(&resource.id());
            }
            _ => {}
        }
    }
}
