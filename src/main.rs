use std::{cell::RefCell, os::fd::AsFd, rc::Rc, sync::Arc};

use drm::control::Device as _;
use gbm::{BufferObjectFlags, Device, Format};
use libseat::Seat;
use nix::time::{ClockId, clock_gettime};
use nix::{poll::PollTimeout, sys::epoll};
use pattern::vulkan::RenderQuad;
use pattern::{
    gpu::{Card, buffer::Buffer},
    input::Input,
    server::{ClientState, ServerState},
    utils,
    vulkan::{VulkanContext, frame::VulkanFrame},
};
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::wp::primary_selection::zv1::server::zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1;
use wayland_protocols::wp::viewporter::server::wp_viewporter::WpViewporter;
use wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use wayland_protocols::xdg::shell::server::xdg_wm_base::XdgWmBase;
use wayland_protocols::xdg::xdg_output::zv1::server::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_server::protocol::wl_data_device_manager::WlDataDeviceManager;
use wayland_server::{
    Display, ListeningSocket, Resource,
    protocol::{
        wl_compositor::WlCompositor, wl_output::WlOutput, wl_seat::WlSeat, wl_shm::WlShm,
        wl_subcompositor::WlSubcompositor,
    },
};

fn main() {
    let seat = Seat::open(|seat, event| match event {
        libseat::SeatEvent::Enable => println!("[seat]: Acquired DRM Master"),
        libseat::SeatEvent::Disable => {
            println!("[seat]: Lost DRM Master (User switched TTY)");
            seat.disable().unwrap();
        }
    })
    .expect("Failed to open libseat. Is seatd or systemd-logind running?");

    for (var, value) in std::env::vars() {
        eprintln!("{var} :: {value}")
    }

    let shared_seat = Rc::new(RefCell::new(seat));

    let card = Card::open(None, shared_seat.clone());
    println!("[info]: {card}");
    println!("[info]: {:?}", card.get_driver().unwrap());

    let info = card.fetch_gpu_info();

    let stat = nix::sys::stat::fstat(card.as_fd()).unwrap();
    let gpu_dev_t = stat.st_rdev as libc::dev_t;

    let table_fd = nix::sys::memfd::memfd_create(
        "dmabuf-formats",
        nix::sys::memfd::MFdFlags::MFD_CLOEXEC | nix::sys::memfd::MFdFlags::MFD_ALLOW_SEALING,
    )
    .unwrap();

    // We have 4 entries now. 4 * 16 bytes = 64 bytes.
    nix::unistd::ftruncate(&table_fd, 64).unwrap();

    let mut table_data = Vec::new();

    // Entry 0: ARGB8888, LINEAR
    table_data.extend_from_slice(&0x34325241u32.to_ne_bytes());
    table_data.extend_from_slice(&0u32.to_ne_bytes());
    table_data.extend_from_slice(&0u64.to_ne_bytes());

    // Entry 1: XRGB8888, LINEAR
    table_data.extend_from_slice(&0x34325258u32.to_ne_bytes());
    table_data.extend_from_slice(&0u32.to_ne_bytes());
    table_data.extend_from_slice(&0u64.to_ne_bytes());

    // Entry 2: ABGR8888, LINEAR
    table_data.extend_from_slice(&0x34324241u32.to_ne_bytes());
    table_data.extend_from_slice(&0u32.to_ne_bytes());
    table_data.extend_from_slice(&0u64.to_ne_bytes());

    // Entry 3: XBGR8888, LINEAR
    table_data.extend_from_slice(&0x34324258u32.to_ne_bytes());
    table_data.extend_from_slice(&0u32.to_ne_bytes());
    table_data.extend_from_slice(&0u64.to_ne_bytes());

    nix::unistd::write(&table_fd, &table_data).unwrap();

    use nix::fcntl::{FcntlArg, SealFlag, fcntl};
    fcntl(
        &table_fd,
        FcntlArg::F_ADD_SEALS(
            SealFlag::F_SEAL_SHRINK
                | SealFlag::F_SEAL_GROW
                | SealFlag::F_SEAL_WRITE
                | SealFlag::F_SEAL_SEAL,
        ),
    )
    .expect("Failed to seal format table");

    let gbm = Device::new(&card).expect("Failed to create GBM device");
    let (width, height) = info.mode.size();

    println!("[info]: {:?}", info.mode);

    println!("[pattern]: Booting Wayland Server...");
    let mut display: Display<ServerState> = Display::new().unwrap();
    let dh = display.handle();

    dh.create_global::<ServerState, WlCompositor, ()>(5, ());
    dh.create_global::<ServerState, WlShm, ()>(1, ());
    dh.create_global::<ServerState, WlSubcompositor, ()>(1, ());
    dh.create_global::<ServerState, WlOutput, ()>(4, ());
    dh.create_global::<ServerState, WlSeat, ()>(5, ());
    dh.create_global::<ServerState, WlDataDeviceManager, ()>(3, ());
    dh.create_global::<ServerState, ZwpPrimarySelectionDeviceManagerV1, ()>(1, ());
    dh.create_global::<ServerState, XdgWmBase, ()>(3, ());
    dh.create_global::<ServerState, ZwpLinuxDmabufV1, ()>(4, ());
    dh.create_global::<ServerState, ZxdgDecorationManagerV1, ()>(1, ());
    dh.create_global::<ServerState, WpViewporter, ()>(1, ());
    dh.create_global::<ServerState, ZxdgOutputManagerV1, ()>(2, ());

    let socket = ListeningSocket::bind_auto("wayland", 0..32).unwrap();
    let socket_name = socket.socket_name().unwrap().to_string_lossy().into_owned();
    println!("[pattern]: Wayland socket created: {}", socket_name);

    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &socket_name);
        std::env::set_var("XDG_SESSION_TYPE", "wayland");
        std::env::set_var("XDG_CURRENT_DESKTOP", "Pattern");
        std::env::set_var("DESKTOP", "Pattern");
        std::env::set_var("DISPLAY", ":0");
    }

    println!("[pattern]: Booting Vulkan Context...");
    let vkctx = Rc::new(VulkanContext::new());
    println!("[pattern]: Vulkan Ready. Entering the void.");

    let mut state = ServerState::new(vkctx.clone(), info.mode.clone(), gpu_dev_t, table_fd);
    let mut input = Input::new(shared_seat.clone(), width as f64, height as f64);
    input.natural_scroll = true; // Change this to false to disable natural scroll

    let epoll = epoll::Epoll::new(epoll::EpollCreateFlags::empty()).unwrap();

    epoll
        .add(&card, epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 0))
        .unwrap();
    epoll
        .add(
            &input.context,
            epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 1),
        )
        .unwrap();
    epoll
        .add(
            shared_seat.borrow_mut().get_fd().unwrap(),
            epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 2),
        )
        .unwrap();
    epoll
        .add(
            socket.as_fd(),
            epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 3),
        )
        .unwrap(); // NEW CLIENTS
    epoll
        .add(
            display.backend().poll_fd(),
            epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 4),
        )
        .unwrap(); // EXISTING CLIENTS

    let mut swapchain: Vec<VulkanFrame> = Vec::with_capacity(2);

    for _ in 0..2 {
        let bo = Buffer::new(
            gbm.create_buffer_object(
                width as u32,
                height as u32,
                Format::Xrgb8888,
                BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
            )
            .unwrap(),
        );

        let fb_handle = card.add_framebuffer(&bo, 24, 32).unwrap();
        let (image, memory) = unsafe { vkctx.import_gbm_buffer(&bo, width as u32, height as u32) };

        let (vk_view, vk_fb) =
            unsafe { vkctx.create_vk_framebuffer(image, width as u32, height as u32) };

        swapchain.push(VulkanFrame {
            bo,
            image,
            memory,
            fb_handle,
            vk_view,
            vk_fb,
        });
    }

    println!("[pattern]: cursor trick");
    let cursor_path = utils::desktop::find_cursor("Adwaita", "left_ptr")
        .or_else(|| utils::desktop::find_cursor("default", "left_ptr"))
        .expect("Could not find a system cursor!");

    let cursor_content = std::fs::read(cursor_path).unwrap();
    let cursor_images = xcursor::parser::parse_xcursor(&cursor_content).unwrap();

    let target_size = 24;

    let cursor_image = cursor_images
        .iter()
        .min_by_key(|img| (img.width as i32 - target_size).abs())
        .unwrap_or(&cursor_images[0]); // Fallback to the first one just in case

    println!(
        "[pattern]: Selected cursor size {}x{}",
        cursor_image.width, cursor_image.height
    );

    let (cursor_vk_img, cursor_vk_mem, cursor_view, cursor_sampler) = unsafe {
        vkctx.upload_texture(
            cursor_image.width,
            cursor_image.height,
            &cursor_image.pixels_rgba,
        )
    };

    let (desc_pool, desc_set) = unsafe {
        vkctx.create_descriptor_set(vkctx.descriptor_set_layout, cursor_view, cursor_sampler)
    };

    let hot_x = cursor_image.xhot as f32;
    let hot_y = cursor_image.yhot as f32;
    let cur_w = cursor_image.width as f32;
    let cur_h = cursor_image.height as f32;

    let mut frame_index = 0;

    let mut waiting_for_flip = false;
    let mut running = true;

    let socket_name = socket.socket_name().unwrap().to_string_lossy().into_owned();
    println!("[pattern]: Auto-launching Kitty on socket: {}", socket_name);

    println!("[pattern]: Started :)");

    while running {
        let timeout = if waiting_for_flip {
            PollTimeout::NONE
        } else {
            PollTimeout::ZERO
        };

        let mut events = [epoll::EpollEvent::empty(); 5];
        let num_events = epoll.wait(&mut events, timeout).unwrap();

        for i in 0..num_events {
            match events[i].data() {
                0 => {
                    let drm_events = card.receive_events().unwrap();
                    for event in drm_events {
                        match event {
                            drm::control::Event::PageFlip(_vblank) => {
                                waiting_for_flip = false;

                                let ts = clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap();
                                let now = (ts.tv_sec() * 1000 + ts.tv_nsec() / 1_000_000) as u32;

                                for cb in state.frame_callbacks.drain(..) {
                                    cb.done(now);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                1 => {
                    if input.dispatch(&mut state, &dh) {
                        running = false;
                    }
                }
                2 => {
                    shared_seat.borrow_mut().dispatch(-1).unwrap();
                }
                3 => {
                    if let Ok(Some(stream)) = socket.accept() {
                        println!("[pattern]: A new Wayland client connected!");
                        display
                            .handle()
                            .insert_client(stream, Arc::new(ClientState))
                            .unwrap();
                    }
                }
                4 => {
                    display.dispatch_clients(&mut state).unwrap();
                }
                _ => unreachable!(),
            }
        }

        if !waiting_for_flip {
            let frame = &swapchain[frame_index % 2];

            let mut dead_surface_ids = Vec::new();

            for win in state.wm.get_render_list() {
                if !win.surface.is_alive() {
                    dead_surface_ids.push(win.surface.id());
                }
            }

            for popup in state.wm.get_popups() {
                if !popup.surface.is_alive() {
                    dead_surface_ids.push(popup.surface.id());
                }
            }

            if let Some((cursor_surf, _, _)) = &state.cursor_surface {
                if !cursor_surf.is_alive() {
                    dead_surface_ids.push(cursor_surf.id());
                    state.cursor_surface = None;
                }
            }

            for id in dead_surface_ids {
                state.wm.unmap_window(&id);
                state.wm.unmap_popup(&id);

                state.xdg_to_surface.retain(|_, v| v.id() != id);

                if let Some(tex) = state.surface_textures.remove(&id) {
                    println!("[pattern]: Client disconnected! Reaping surface memory...");
                    unsafe {
                        vkctx.device.destroy_sampler(tex.samp, None);
                        vkctx.device.destroy_image_view(tex.view, None);
                        vkctx.device.destroy_image(tex.img, None);
                        vkctx.device.free_memory(tex.mem, None);
                        vkctx.device.destroy_descriptor_pool(tex.pool, None);
                    }
                }
            }

            state.windows.retain(|w| w.is_alive());
            state.outputs.retain(|o| o.is_alive());
            state.pointers.retain(|p| p.is_alive());
            state.keyboards.retain(|k| k.is_alive());

            state.data_devices.retain(|d| d.is_alive());
            state.primary_selection_devices.retain(|d| d.is_alive());
            state.data_sources.retain(|_, (s, _)| s.is_alive());
            state
                .primary_selection_sources
                .retain(|_, (s, _)| s.is_alive());

            if let Some(focus) = &state.input_focus {
                if !focus.is_alive() {
                    state.input_focus = None;
                }
            }
            if let Some(focus) = &state.pointer_focus {
                if !focus.is_alive() {
                    state.pointer_focus = None;
                }
            }

            let mut draw_list = state.styler.generate_draw_list(
                &state.wm.get_render_list(),
                &state.wm.get_popups(),
                &state.subsurfaces,
                &state.surface_textures,
                &state.viewports,
                &state.surface_to_viewport,
                &state.surface_opaque_region,
                state.wm.as_ref(),
            );

            let mut cursor_drawn = false;

            if let Some((cursor_surf, hot_x, hot_y)) = &state.cursor_surface {
                if let Some(tex) = state.surface_textures.get(&cursor_surf.id()) {
                    draw_list.push(pattern::vulkan::DrawCommand::Texture(RenderQuad {
                        set: tex.set,
                        x: (input.cursor.x as f32 - *hot_x as f32).round(),
                        y: (input.cursor.y as f32 - *hot_y as f32).round(),
                        w: tex.w,
                        h: tex.h,
                        src_x: 0.0,
                        src_y: 0.0,
                        src_w: 1.0,
                        src_h: 1.0,
                        border_radius: 0.0,
                    }));
                    cursor_drawn = true;
                }
            }

            if !cursor_drawn {
                draw_list.push(pattern::vulkan::DrawCommand::Texture(RenderQuad {
                    set: desc_set,
                    x: (input.cursor.x as f32 - hot_x).round(),
                    y: (input.cursor.y as f32 - hot_y).round(),
                    w: cur_w,
                    h: cur_h,
                    src_x: 0.0,
                    src_y: 0.0,
                    src_w: 1.0,
                    src_h: 1.0,
                    border_radius: 0.0,
                }));
            }

            unsafe {
                vkctx.draw_frame(frame.vk_fb, width as u32, height as u32, &draw_list);
            }

            card.page_flip(
                info.crtc_handle,
                frame.fb_handle,
                drm::control::PageFlipFlags::EVENT,
                None, // No user data for now
            )
            .expect("Failed to page flip");

            waiting_for_flip = true;
            frame_index += 1;
        }

        let _ = display.flush_clients();
    }

    println!("[pattern]: Tearing down swapchain...");
    for frame in swapchain {
        unsafe { frame.destroy(&vkctx.device, &card) };
    }

    for tex in state.surface_textures.values() {
        unsafe {
            vkctx.device.destroy_sampler(tex.samp, None);
            vkctx.device.destroy_image_view(tex.view, None);
            vkctx.device.destroy_image(tex.img, None);
            vkctx.device.free_memory(tex.mem, None);
            vkctx.device.destroy_descriptor_pool(tex.pool, None);
        }
    }

    unsafe {
        vkctx.device.destroy_sampler(cursor_sampler, None);
        vkctx.device.destroy_image_view(cursor_view, None);
        vkctx.device.destroy_image(cursor_vk_img, None);
        vkctx.device.free_memory(cursor_vk_mem, None);

        vkctx.device.destroy_descriptor_pool(desc_pool, None);

        vkctx
            .device
            .destroy_descriptor_set_layout(vkctx.descriptor_set_layout, None);
        vkctx.device.destroy_pipeline(vkctx.graphics_pipeline, None);
        vkctx.device.destroy_pipeline(vkctx.color_pipeline, None);
        vkctx
            .device
            .destroy_pipeline_layout(vkctx.pipeline_layout, None);
        vkctx
            .device
            .destroy_pipeline_layout(vkctx.color_pipeline_layout, None);
        vkctx.device.destroy_render_pass(vkctx.render_pass, None);

        vkctx.device.destroy_fence(vkctx.fence, None);
        vkctx.device.destroy_command_pool(vkctx.command_pool, None);
        vkctx.device.destroy_device(None);
        vkctx.instance.destroy_instance(None);
    }

    println!("[pattern]: Engine shut down safely. Returning to the terminal.");
}
