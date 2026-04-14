use crate::server::ServerState;
use wayland_server::protocol::{
    wl_callback::WlCallback, wl_compositor::WlCompositor, wl_region::WlRegion,
    wl_surface::WlSurface,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlCompositor, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlCompositor>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlCompositor, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlCompositor,
        request: wayland_server::protocol::wl_compositor::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_compositor::Request::CreateSurface { id } => {
                let surface = data_init.init(id, ());
                println!("[pattern]: Client requested a new surface!");

                for output in &state.outputs {
                    if surface.client().map(|c| c.id()) == output.client().map(|c| c.id()) {
                        surface.enter(output);
                    }
                }

                state.surfaces.push(surface);
            }
            wayland_server::protocol::wl_compositor::Request::CreateRegion { id } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSurface, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        surface: &WlSurface,
        request: wayland_server::protocol::wl_surface::Request,
        _data: &(),
        dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        use crate::vulkan::SurfaceTexture;
        use ash::vk;

        match request {
            wayland_server::protocol::wl_surface::Request::Attach { buffer, .. } => {
                if let Some(buf) = buffer {
                    state.surface_buffers.insert(surface.id(), buf.clone());
                }
            }
            wayland_server::protocol::wl_surface::Request::Commit => {
                if let Some(buffer) = state.surface_buffers.remove(&surface.id()) {
                    if let Some(buffer_info) = state.buffers.get(&buffer.id()) {
                        if let Some((_, mmap)) = state.pools.get(&buffer_info.pool_id) {
                            let start = buffer_info.offset as usize;
                            let len = (buffer_info.height * buffer_info.stride) as usize;
                            let pixels = &mmap[start..start + len];

                            unsafe {
                                if let Some(old) = state.surface_textures.remove(&surface.id()) {
                                    state.vkctx.device.destroy_sampler(old.samp, None);
                                    state.vkctx.device.destroy_image_view(old.view, None);
                                    state.vkctx.device.destroy_image(old.img, None);
                                    state.vkctx.device.free_memory(old.mem, None);
                                    state.vkctx.device.destroy_descriptor_pool(old.pool, None);
                                }

                                let (img, mem, view, samp) = state.vkctx.upload_texture(
                                    buffer_info.width as u32,
                                    buffer_info.height as u32,
                                    pixels,
                                );

                                let (pool, set) = state.vkctx.create_descriptor_set(
                                    state.vkctx.descriptor_set_layout,
                                    view,
                                    samp,
                                );

                                #[rustfmt::skip]
                                state.surface_textures.insert(
                                    surface.id(),
                                    SurfaceTexture {
                                        img, mem, view, samp, pool, set,
                                        w: buffer_info.width as f32,
                                        h: buffer_info.height as f32,
                                        scale: *state.pending_scales.get(&surface.id()).unwrap_or(&1),
                                    },
                                );
                                state.wm.refresh_window_dimensions(
                                    &surface.id(),
                                    buffer_info.width,
                                    buffer_info.height,
                                );
                            }
                        }
                    } else if let Some(dmabuf) = state.dmabuffers.get(&buffer.id()) {
                        unsafe {
                            if let Some(old) = state.surface_textures.remove(&surface.id()) {
                                state.vkctx.device.destroy_sampler(old.samp, None);
                                state.vkctx.device.destroy_image_view(old.view, None);
                                state.vkctx.device.destroy_image(old.img, None);
                                state.vkctx.device.free_memory(old.mem, None);
                                state.vkctx.device.destroy_descriptor_pool(old.pool, None);
                            }

                            let (img, mem) = state.vkctx.import_dmabuf(
                                &dmabuf.fd,
                                dmabuf.width,
                                dmabuf.height,
                                dmabuf.stride,
                                dmabuf.modifier,
                                dmabuf.format,
                            );

                            let format = match dmabuf.format {
                                0x34324241 | 0x34324258 => vk::Format::R8G8B8A8_UNORM,
                                _ => vk::Format::B8G8R8A8_UNORM,
                            };

                            let view_info = vk::ImageViewCreateInfo::default()
                                .image(img)
                                .view_type(vk::ImageViewType::TYPE_2D)
                                .format(format)
                                .subresource_range(
                                    vk::ImageSubresourceRange::default()
                                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                                        .level_count(1)
                                        .layer_count(1),
                                );

                            let view = state
                                .vkctx
                                .device
                                .create_image_view(&view_info, None)
                                .expect("Failed to create Image View for DMA-BUF");

                            let sampler_info = vk::SamplerCreateInfo::default()
                                .mag_filter(vk::Filter::LINEAR)
                                .min_filter(vk::Filter::LINEAR)
                                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);

                            let samp = state
                                .vkctx
                                .device
                                .create_sampler(&sampler_info, None)
                                .expect("Failed to create Sampler for DMA-BUF");

                            let (pool, set) = state.vkctx.create_descriptor_set(
                                state.vkctx.descriptor_set_layout,
                                view,
                                samp,
                            );

                            state.surface_textures.insert(
                                surface.id(),
                                SurfaceTexture {
                                    img,
                                    mem,
                                    view,
                                    samp,
                                    pool,
                                    set,
                                    w: dmabuf.width as f32,
                                    h: dmabuf.height as f32,
                                    scale: *state.pending_scales.get(&surface.id()).unwrap_or(&1),
                                },
                            );
                            state.wm.refresh_window_dimensions(
                                &surface.id(),
                                dmabuf.width as i32,
                                dmabuf.height as i32,
                            );
                        }
                    }

                    buffer.release();
                }
            }
            wayland_server::protocol::wl_surface::Request::Frame { callback } => {
                let cb = data_init.init(callback, ());
                state.frame_callbacks.push(cb);
            }
            wayland_server::protocol::wl_surface::Request::SetBufferScale { scale } => {
                state.pending_scales.insert(surface.id(), scale);
            }
            wayland_server::protocol::wl_surface::Request::Destroy => {
                let is_focused = state.input_focus.as_ref().map(|s| s.id()) == Some(surface.id());
                if is_focused {
                    state.input_focus = None;
                }
                if state.pointer_focus.as_ref().map(|s| s.id()) == Some(surface.id()) {
                    state.pointer_focus = None;
                }
                state.wm.unmap_window(&surface.id());
                state.wm.unmap_popup(&surface.id());
                state.surfaces.retain(|s| s.id() != surface.id());
                if let Some(vp_id) = state.surface_to_viewport.remove(&surface.id()) {
                    state.viewports.remove(&vp_id);
                }

                if is_focused {
                    if let Some(next_window) = state.wm.get_render_list().last() {
                        state.set_input_focus(next_window.surface.clone(), dhandle);
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlCallback, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlCallback,
        _request: wayland_server::protocol::wl_callback::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<WlRegion, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlRegion,
        _request: wayland_server::protocol::wl_region::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}
