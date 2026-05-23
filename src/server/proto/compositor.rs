use crate::server::Composer;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use wayland_backend::server::ObjectId;
use wayland_server::protocol::{
    wl_callback::WlCallback, wl_compositor::WlCompositor, wl_region::WlRegion,
    wl_surface::WlSurface,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlCompositor, ()> for Composer {
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

impl Dispatch<WlCompositor, ()> for Composer {
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

                for output in &state.outputs {
                    if surface.client().map(|c| c.id()) == output.client().map(|c| c.id()) {
                        surface.enter(output);
                    }
                }

                state.surfaces.push(surface);
            }
            wayland_server::protocol::wl_compositor::Request::CreateRegion { id } => {
                let region = data_init.init(id, ());
                state.regions.insert(region.id(), Vec::new());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSurface, ()> for Composer {
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
        use crate::wm::Rect;
        use ash::vk;

        match request {
            wayland_server::protocol::wl_surface::Request::Attach { buffer, .. } => {
                if let Some(buf) = buffer {
                    state.surface_buffers.insert(surface.id(), buf.clone());
                }
            }
            wayland_server::protocol::wl_surface::Request::Damage {
                x,
                y,
                width,
                height,
            } => {
                let scale = *state.pending_scales.get(&surface.id()).unwrap_or(&1);
                state
                    .pending_damage
                    .entry(surface.id())
                    .or_default()
                    .push(Rect {
                        x: x * scale,
                        y: y * scale,
                        w: width * scale,
                        h: height * scale,
                    });
            }
            wayland_server::protocol::wl_surface::Request::DamageBuffer {
                x,
                y,
                width,
                height,
            } => {
                state
                    .pending_damage
                    .entry(surface.id())
                    .or_default()
                    .push(Rect {
                        x,
                        y,
                        w: width,
                        h: height,
                    });
            }
            wayland_server::protocol::wl_surface::Request::SetInputRegion { region } => {
                if let Some(reg) = region {
                    if let Some(rects) = state.regions.get(&reg.id()) {
                        state
                            .pending_input_region
                            .insert(surface.id(), Some(rects.clone()));
                    }
                } else {
                    state.pending_input_region.insert(surface.id(), None);
                }
            }
            wayland_server::protocol::wl_surface::Request::SetOpaqueRegion { region } => {
                if let Some(reg) = region {
                    if let Some(rects) = state.regions.get(&reg.id()) {
                        state
                            .pending_opaque_region
                            .insert(surface.id(), Some(rects.clone()));
                    }
                } else {
                    state.pending_opaque_region.insert(surface.id(), None);
                }
            }
            wayland_server::protocol::wl_surface::Request::Commit => {
                if let Some(input_opt) = state.pending_input_region.remove(&surface.id()) {
                    if let Some(input) = input_opt {
                        state.surface_input_region.insert(surface.id(), input);
                    } else {
                        state.surface_input_region.remove(&surface.id());
                    }
                }
                if let Some(opaque_opt) = state.pending_opaque_region.remove(&surface.id()) {
                    if let Some(opaque) = opaque_opt {
                        state.surface_opaque_region.insert(surface.id(), opaque);
                    } else {
                        state.surface_opaque_region.remove(&surface.id());
                    }
                }

                let mut actual_size = None;
                if let Some(buffer) = state.surface_buffers.get(&surface.id()) {
                    if let Some(buffer_info) = state.buffers.get(&buffer.id()) {
                        actual_size = Some((buffer_info.width, buffer_info.height));
                    } else if let Some(dmabuf) = state.dmabuffers.get(&buffer.id()) {
                        actual_size = Some((dmabuf.width as i32, dmabuf.height as i32));
                    }
                }

                let (actual_w, actual_h) = actual_size.unwrap_or_else(|| {
                    state
                        .wm
                        .all_windows()
                        .iter()
                        .find(|w| w.surface.id() == surface.id())
                        .map(|w| (w.w, w.h))
                        .unwrap_or((0, 0))
                });

                if let Some(geometry) = state.pending_geometry.remove(&surface.id()) {
                    state.wm.set_window_geometry(&surface.id(), geometry);
                }

                state
                    .wm
                    .apply_committed_configure(&surface.id(), actual_w, actual_h);

                if let Some((x, y)) = state.pending_popup_positions.remove(&surface.id()) {
                    state.wm.update_popup_position(&surface.id(), x, y);
                }

                if let Some(layer_state) = state.pending_layer_state.remove(&surface.id()) {
                    if let Some(size) = layer_state.size {
                        state
                            .wm
                            .set_layer_surface_size(&surface.id(), size.0, size.1);
                    }
                    if let Some(anchor) = layer_state.anchor {
                        state.wm.set_layer_surface_anchor(&surface.id(), anchor);
                    }
                    if let Some(zone) = layer_state.zone {
                        state.wm.set_layer_surface_zone(&surface.id(), zone);
                    }
                    if let Some(margin) = layer_state.margin {
                        state.wm.set_layer_surface_margin(
                            &surface.id(),
                            margin.0,
                            margin.1,
                            margin.2,
                            margin.3,
                        );
                    }
                    if let Some(interactivity) = layer_state.interactivity {
                        state
                            .wm
                            .set_layer_keyboard_interactivity(&surface.id(), interactivity);
                    }
                    state.wm.recalculate_layer_layout(state.mode.size());
                }

                // Apply subsurface positions (parent-commit double-buffered)
                let sub_ids: Vec<ObjectId> = state
                    .subsurfaces
                    .iter()
                    .filter(|s| s.parent.id() == surface.id())
                    .map(|s| s.id.clone())
                    .collect();

                for sub_id in sub_ids {
                    if let Some((x, y)) = state.pending_subsurface_positions.remove(&sub_id) {
                        if let Some(sub) = state.subsurfaces.iter_mut().find(|s| s.id == sub_id) {
                            sub.x = x;
                            sub.y = y;
                        }
                    }
                }

                // Track if we had a previous sync state for delayed release
                if let Some(new_sync) = state.pending_syncobj_state.remove(&surface.id()) {
                    let sync_state = state.syncobj_state.entry(surface.id()).or_default();

                    // The previous "current" buffer is now being replaced.
                    // Move its release point to the signal queue.
                    if let Some(old_release) = sync_state.current_release.take() {
                        sync_state.signal_queue.push(old_release);
                    }

                    // Apply new state
                    sync_state.acquire_point = new_sync.acquire_point;
                    sync_state.current_release = new_sync.current_release;
                    sync_state.signal_queue.extend(new_sync.signal_queue);
                }

                // Handle presentation feedbacks
                if let Some(new_feedbacks) =
                    state.pending_presentation_feedbacks.remove(&surface.id())
                {
                    if let Some(old_feedbacks) = state
                        .surface_presentation_feedbacks
                        .insert(surface.id(), new_feedbacks)
                    {
                        for fb in old_feedbacks {
                            fb.discarded();
                        }
                    }
                }

                // Handle frame callbacks
                if let Some(new_callbacks) = state.pending_frame_callbacks.remove(&surface.id()) {
                    state.active_frame_callbacks.extend(new_callbacks);
                }

                state.needs_redraw = true;

                let damage = state
                    .pending_damage
                    .remove(&surface.id())
                    .unwrap_or_default();

                if let Some(buffer) = state.surface_buffers.remove(&surface.id()) {
                    let scale = *state.pending_scales.get(&surface.id()).unwrap_or(&1);

                    if let Some(buffer_info) = state.buffers.get(&buffer.id()) {
                        unsafe {
                            let tex = if let Some(cached_tex) =
                                state.buffer_textures.get(&buffer.id())
                            {
                                cached_tex.clone_with_scale(scale as f32)
                            } else {
                                if let Some((_, mmap)) = state.pools.get(&buffer_info.pool_id) {
                                    let start = buffer_info.offset as usize;
                                    let len = (buffer_info.height * buffer_info.stride) as usize;
                                    if len == 0 || start + len > mmap.len() {
                                        return;
                                    }
                                    let pixels = &mmap[start..start + len];
                                    let (img, mem, view, samp) = state.vkctx.upload_texture(
                                        buffer_info.width as u32,
                                        buffer_info.height as u32,
                                        buffer_info.stride as u32,
                                        pixels,
                                    );

                                    let (pool, set) = state.vkctx.create_descriptor_set(
                                        state.vkctx.descriptor_set_layout,
                                        view,
                                        samp,
                                    );

                                    let inner = Arc::new(crate::vulkan::VulkanTextureInner {
                                        device: state.vkctx.device.clone(),
                                        img,
                                        mem,
                                        view,
                                        samp,
                                        pool,
                                    });

                                    let new_tex = SurfaceTexture {
                                        inner,
                                        set,
                                        w: buffer_info.width as f32,
                                        h: buffer_info.height as f32,
                                        scale: scale as f32,
                                    };

                                    state.buffer_textures.insert(buffer.id(), new_tex.clone());
                                    new_tex
                                } else {
                                    return;
                                }
                            };

                            // Update SHM content if damaged. NO POLL NEEDED FOR SHM.
                            if !damage.is_empty() {
                                if let Some((_, mmap)) = state.pools.get(&buffer_info.pool_id) {
                                    let start = buffer_info.offset as usize;
                                    let len = (buffer_info.height * buffer_info.stride) as usize;
                                    if len == 0 || start + len > mmap.len() {
                                        return;
                                    }
                                    let pixels = &mmap[start..start + len];
                                    state.vkctx.update_texture(
                                        tex.inner.img,
                                        buffer_info.width as u32,
                                        buffer_info.height as u32,
                                        buffer_info.stride as u32,
                                        pixels,
                                        &damage,
                                    );
                                }
                            }

                            state.surface_textures.insert(surface.id(), tex);
                            state.wm.refresh_window_dimensions(
                                &surface.id(),
                                buffer_info.width,
                                buffer_info.height,
                            );
                        }

                        // Buffer lifecycle: Only release the PREVIOUS buffer.
                        // Holding the current one ensures the client doesn't overwrite it while we draw.
                        if !state.explicit_sync_surfaces.contains(&surface.id()) {
                            if let Some(old_buffer) =
                                state.active_dmabufs.insert(surface.id(), buffer.clone())
                            {
                                if old_buffer.id() != buffer.id() {
                                    old_buffer.release();
                                }
                            }
                        } else {
                            state.active_dmabufs.insert(surface.id(), buffer.clone());
                        }
                    } else if let Some(dmabuf) = state.dmabuffers.get(&buffer.id()) {
                        unsafe {
                            let has_acquire_point = state
                                .pending_syncobj_state
                                .get(&surface.id())
                                .and_then(|s| s.acquire_point)
                                .is_some();

                            let tex =
                                if let Some(cached_tex) = state.buffer_textures.get(&buffer.id()) {
                                    cached_tex.clone_with_scale(scale as f32)
                                } else {
                                    let (img, mem) = state.vkctx.import_dmabuf(
                                        &dmabuf.fd,
                                        dmabuf.width,
                                        dmabuf.height,
                                        dmabuf.offset,
                                        dmabuf.stride,
                                        dmabuf.modifier,
                                        dmabuf.format,
                                        !has_acquire_point,
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
                                        .expect("Fail View");

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
                                        .expect("Fail Sampler");

                                    let (pool, set) = state.vkctx.create_descriptor_set(
                                        state.vkctx.descriptor_set_layout,
                                        view,
                                        samp,
                                    );

                                    let inner = Arc::new(crate::vulkan::VulkanTextureInner {
                                        device: state.vkctx.device.clone(),
                                        img,
                                        mem,
                                        view,
                                        samp,
                                        pool,
                                    });

                                    let new_tex = SurfaceTexture {
                                        inner,
                                        set,
                                        w: dmabuf.width as f32,
                                        h: dmabuf.height as f32,
                                        scale: scale as f32,
                                    };

                                    state.buffer_textures.insert(buffer.id(), new_tex.clone());
                                    new_tex
                                };

                            // Synchronization: Only poll if NOT using explicit sync
                            if !has_acquire_point {
                                let mut pollfd = libc::pollfd {
                                    fd: dmabuf.fd.as_raw_fd(),
                                    events: libc::POLLIN,
                                    revents: 0,
                                };
                                libc::poll(&mut pollfd, 1, 100);
                            }

                            state.surface_textures.insert(surface.id(), tex);
                            state.wm.refresh_window_dimensions(
                                &surface.id(),
                                dmabuf.width as i32,
                                dmabuf.height as i32,
                            );
                        }

                        if !state.explicit_sync_surfaces.contains(&surface.id()) {
                            if let Some(old_buffer) =
                                state.active_dmabufs.insert(surface.id(), buffer.clone())
                            {
                                if old_buffer.id() != buffer.id() {
                                    old_buffer.release();
                                }
                            }
                        } else {
                            state.active_dmabufs.insert(surface.id(), buffer.clone());
                        }
                    }
                }
            }
            wayland_server::protocol::wl_surface::Request::Frame { callback } => {
                let cb = data_init.init(callback, ());
                state
                    .pending_frame_callbacks
                    .entry(surface.id())
                    .or_default()
                    .push(cb);
            }
            wayland_server::protocol::wl_surface::Request::SetBufferScale { scale } => {
                state.pending_scales.insert(surface.id(), scale);
            }
            wayland_server::protocol::wl_surface::Request::Destroy => {
                state.cleanup_surface(&surface.id(), dhandle);
            }
            _ => {}
        }
    }
}

impl Dispatch<WlCallback, ()> for Composer {
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

impl Dispatch<WlRegion, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlRegion,
        request: wayland_server::protocol::wl_region::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        use crate::wm::Rect;
        match request {
            wayland_server::protocol::wl_region::Request::Add {
                x,
                y,
                width,
                height,
            } => {
                if let Some(rects) = state.regions.get_mut(&resource.id()) {
                    rects.push(Rect {
                        x,
                        y,
                        w: width,
                        h: height,
                    });
                }
            }
            wayland_server::protocol::wl_region::Request::Subtract {
                x,
                y,
                width,
                height,
            } => {
                if let Some(rects) = state.regions.get_mut(&resource.id()) {
                    rects.retain(|r| {
                        !(r.x >= x && r.y >= y && r.x + r.w <= x + width && r.y + r.h <= y + height)
                    });
                }
            }
            wayland_server::protocol::wl_region::Request::Destroy => {
                state.regions.remove(&resource.id());
            }
            _ => {}
        }
    }
}
