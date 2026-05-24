use std::collections::HashSet;
use std::io::IsTerminal;
use std::{cell::RefCell, os::fd::AsFd, rc::Rc, sync::Arc};

use ash::vk;
use drm::control::Device as _;
use drm::control::atomic::AtomicModeReq;
use drm::control::AtomicCommitFlags;
use gbm::{BufferObjectFlags, Device, Format};
use libseat::Seat;
use nix::{poll::PollTimeout, sys::epoll};
use pattern::config::ConfigManager;
use pattern::styler;
use pattern::vulkan::{DrawCommand, RenderQuad};
use pattern::wm::impls::floating_wm;
use pattern::{
    gpu::{Card, buffer::Buffer},
    input::Input,
    server::{ClientState, Composer},
    vulkan::{VulkanContext, frame::VulkanFrame},
};
use tracing::{debug, info, error};
use wayland_protocols::wp::fifo::v1::server::wp_fifo_manager_v1::WpFifoManagerV1;
use wayland_protocols::wp::presentation_time::server::wp_presentation::WpPresentation;
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use wayland_protocols::wp::cursor_shape::v1::server::wp_cursor_shape_device_v1;
use wayland_protocols::wp::cursor_shape::v1::server::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1;
use wayland_protocols::wp::pointer_constraints::zv1::server::zwp_pointer_constraints_v1::ZwpPointerConstraintsV1;
use wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1;
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1::WpPointerWarpV1;
use wayland_protocols::wp::primary_selection::zv1::server::zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1;
use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::viewporter::server::wp_viewporter::WpViewporter;
use wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use wayland_protocols::xdg::shell::server::xdg_wm_base::XdgWmBase;
use wayland_protocols::xdg::xdg_output::zv1::server::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols::wp::pointer_gestures::zv1::server::zwp_pointer_gestures_v1::ZwpPointerGesturesV1;
use wayland_protocols::ext::workspace::v1::server::ext_workspace_manager_v1::ExtWorkspaceManagerV1;
use wayland_protocols::xdg::dialog::v1::server::xdg_wm_dialog_v1::XdgWmDialogV1;
use wayland_protocols::xdg::activation::v1::server::xdg_activation_v1::XdgActivationV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::server::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_wlr::data_control::v1::server::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::gamma_control::v1::server::zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1;
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use wayland_server::protocol::wl_data_device_manager::WlDataDeviceManager;
use wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_manager_v1::WpLinuxDrmSyncobjManagerV1;
use wayland_protocols_misc::zwp_input_method_v2::server::zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
use wayland_server::{
    Display, ListeningSocket, Resource,
    protocol::{
        wl_compositor::WlCompositor, wl_output::WlOutput, wl_seat::WlSeat, wl_shm::WlShm,
        wl_subcompositor::WlSubcompositor,
    },
};

fn main() {
    let is_tty = std::io::stdout().is_terminal();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_thread_ids(true)
        .with_ansi(is_tty)
        .with_timer(tracing_subscriber::fmt::time::time())
        .init();

    let seat = Seat::open(|seat, event| match event {
        libseat::SeatEvent::Enable => info!("[seat] Acquired DRM Master"),
        libseat::SeatEvent::Disable => {
            info!("[seat] Lost DRM Master (User switched TTY)");
            seat.disable().unwrap();
        }
    })
    .expect("Failed to open libseat. Is seatd or systemd-logind running?");

    let shared_seat = Rc::new(RefCell::new(seat));

    let card = Card::open(None, shared_seat.clone());
    info!("{card}");
    info!("{:?}", card.get_driver().unwrap());

    let info = card.fetch_card_info();

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

    info!("{:?}", info.mode);

    info!("Booting Wayland Server");
    let mut display: Display<Composer> = Display::new().unwrap();
    let dh = display.handle();

    dh.create_global::<Composer, WlCompositor, ()>(5, ());
    dh.create_global::<Composer, WlShm, ()>(1, ());
    dh.create_global::<Composer, WlSubcompositor, ()>(1, ());
    dh.create_global::<Composer, WlOutput, ()>(4, ());
    dh.create_global::<Composer, WlSeat, ()>(5, ());
    dh.create_global::<Composer, WlDataDeviceManager, ()>(3, ());
    dh.create_global::<Composer, ZwpPrimarySelectionDeviceManagerV1, ()>(1, ());
    dh.create_global::<Composer, XdgWmBase, ()>(3, ());
    dh.create_global::<Composer, ZwpLinuxDmabufV1, ()>(5, ());
    dh.create_global::<Composer, ZxdgDecorationManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpViewporter, ()>(1, ());
    dh.create_global::<Composer, ZxdgOutputManagerV1, ()>(2, ());
    dh.create_global::<Composer, ZwlrLayerShellV1, ()>(4, ());
    dh.create_global::<Composer, ExtWorkspaceManagerV1, ()>(1, ());
    dh.create_global::<Composer, XdgWmDialogV1, ()>(1, ());
    dh.create_global::<Composer, XdgActivationV1, ()>(1, ());
    dh.create_global::<Composer, ZwpPointerGesturesV1, ()>(3, ());
    dh.create_global::<Composer, WpCursorShapeManagerV1, ()>(2, ());
    dh.create_global::<Composer, WpPointerWarpV1, ()>(1, ());
    dh.create_global::<Composer, ZwpPointerConstraintsV1, ()>(1, ());
    dh.create_global::<Composer, ZwpRelativePointerManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpLinuxDrmSyncobjManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpFifoManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpPresentation, ()>(2, ());
    dh.create_global::<Composer, ZwpTextInputManagerV3, ()>(1, ());
    dh.create_global::<Composer, ZwpInputMethodManagerV2, ()>(1, ());
    dh.create_global::<Composer, ZwpVirtualKeyboardManagerV1, ()>(1, ());
    dh.create_global::<Composer, ZwlrDataControlManagerV1, ()>(2, ());
    dh.create_global::<Composer, ZwlrGammaControlManagerV1, ()>(1, ());

    let socket = ListeningSocket::bind_auto("wayland", 0..32).unwrap();
    let socket_name = socket.socket_name().unwrap().to_string_lossy().into_owned();
    info!("Wayland socket created: {}", socket_name);

    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &socket_name);
        std::env::set_var("XDG_SESSION_TYPE", "wayland");
        std::env::set_var("XDG_CURRENT_DESKTOP", "Pattern");
        std::env::set_var("DESKTOP", "Pattern");
        // std::env::set_var("DISPLAY", ":0"); TODO: XWayland
    }

    info!("Creating Vulkan Context");
    let vkctx = Rc::new(VulkanContext::new());
    info!("Vulkan Ready. Entering the void.");

    let mut config_manager = ConfigManager::new(None).expect("Unable to activate the manager");
    config_manager.load().expect("Unable to load configuration");

    let mut composer = Composer::init(
        vkctx.clone(),
        info.mode.clone(),
        info.clone(),
        gpu_dev_t,
        table_fd,
        Box::new(floating_wm::Wm::new()),
        Box::new(styler::DefaultStyler::new()),
        config_manager,
    );

    let initial_style = {
        let cfg = composer.config_manager.config.lock().unwrap();
        cfg.style.clone()
    };
    composer.styler.update_style(initial_style);

    composer
        .config_manager
        .run_hook("@start")
        .unwrap_or_else(|e| {
            error!("Failed to run @start hook: {:?}", e);
        });

    let mut input = Input::new(shared_seat.clone(), width as f64, height as f64);
    input.natural_scroll = {
        let cfg = composer.config_manager.config.lock().unwrap();
        cfg.input.natural_scroll
    };

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

        let blur_chain = unsafe { vkctx.create_blur_chain(3, width as u32, height as u32) };

        swapchain.push(VulkanFrame {
            bo,
            image,
            memory,
            fb_handle,
            vk_view,
            vk_fb,
            blur_target: Some(blur_chain),
        });
    }

    let mut frame_index = 0;
    let mut initial_modeset = true;

    let mut waiting_for_flip = false;
    let mut running = true;
    let mut active_gamma_blob: Option<u64> = None;

    debug!("Started :)");

    while running {
        let timeout = if waiting_for_flip {
            PollTimeout::NONE
        } else if composer.needs_redraw {
            PollTimeout::ZERO
        } else {
            PollTimeout::NONE
        };

        let mut events = [epoll::EpollEvent::empty(); 5];
        let num_events = match epoll.wait(&mut events, timeout) {
            Ok(n) => n,
            Err(e) if e == nix::errno::Errno::EINTR => 0,
            Err(e) => {
                error!("epoll_wait failed: {}", e);
                0
            }
        };

        for i in 0..num_events {
            match events[i].data() {
                0 => {
                    // trace!("DRM Event");
                    let drm_events = card.receive_events().unwrap();
                    for event in drm_events {
                        match event {
                            drm::control::Event::PageFlip(v) => {
                                waiting_for_flip = false;

                                let now = pattern::utils::time::gettime();
                                let tv_sec = (v.duration.as_micros() / 1_000_000) as u64;
                                let tv_nsec = (v.duration.as_micros() % 1_000_000) as u32 * 1000;
                                let seq = v.frame as u64;

                                for cb in composer.active_frame_callbacks.drain(..) {
                                    cb.done(now);
                                }

                                for fb in composer.feedbacks_to_present.drain(..) {
                                    if let Some(client) = fb.client() {
                                        if let Some(output) = composer.outputs.iter().find(|o| {
                                            o.client().map(|c| c.id()) == Some(client.id())
                                        }) {
                                            fb.sync_output(output);
                                        }
                                    }
                                    fb.presented(
                                        (tv_sec >> 32) as u32,
                                        (tv_sec & 0xFFFFFFFF) as u32,
                                        tv_nsec,
                                        ((1. / info.mode.vrefresh() as f64) * 1_000_000.0 * 1_000.0)
                                            as u32, // refresh in ns
                                        (seq >> 32) as u32,
                                        (seq & 0xFFFFFFFF) as u32,
                                        wp_presentation_feedback::Kind::Vsync,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
                1 => {
                    if input.dispatch(&mut composer, &dh) {
                        running = false;
                    }
                    composer.needs_redraw = true;
                }
                2 => {
                    shared_seat.borrow_mut().dispatch(-1).unwrap();
                    composer.needs_redraw = true;
                }
                3 => {
                    if let Ok(Some(stream)) = socket.accept() {
                        display
                            .handle()
                            .insert_client(stream, Arc::new(ClientState))
                            .unwrap();
                    }
                }
                4 => {
                    display.dispatch_clients(&mut composer).unwrap();
                }
                _ => unreachable!(),
            }
        }

        composer.process_config_commands(&dh);

        if !waiting_for_flip && composer.needs_redraw {
            let now = pattern::utils::time::gettime();
            let style = {
                let cfg = composer.config_manager.config.lock().unwrap();
                cfg.style.clone()
            };
            composer.styler.update_style(style);
            let animating = composer.styler.tick(
                now as f64,
                composer.wm.as_ref(),
                &composer.surface_textures,
                composer.mode.size(),
            );
            composer.needs_redraw = animating;

            // debug!("Rendering frame {}", frame_index);
            let frame = &swapchain[frame_index % 2];

            let mut dead_surface_ids = Vec::new();

            for win in composer.wm.all_windows() {
                if !win.surface.is_alive() {
                    dead_surface_ids.push(win.surface.id());
                }
            }

            for popup in composer.wm.get_popups() {
                if !popup.surface.is_alive() {
                    dead_surface_ids.push(popup.surface.id());
                }
            }

            if let Some((cursor_surf, _, _)) = &composer.cursor_surface {
                if !cursor_surf.is_alive() {
                    dead_surface_ids.push(cursor_surf.id());
                    composer.cursor_surface = None;
                }
            }

            for id in dead_surface_ids {
                composer.cleanup_surface(&id, &dh);
            }

            composer.windows.retain(|w| w.is_alive());
            composer.outputs.retain(|o| o.is_alive());
            composer.pointers.retain(|p| p.is_alive());
            composer.keyboards.retain(|k| k.is_alive());
            composer.input_methods.retain(|(im, _)| im.is_alive());
            composer.input_method_grabs.retain(|(g, _)| g.is_alive());
            composer.text_inputs.retain(|(ti, _, _)| ti.is_alive());
            composer
                .input_popups
                .retain(|(p, s, im)| p.is_alive() && s.is_alive() && im.is_alive());

            composer.data_devices.retain(|d| d.is_alive());
            composer.primary_selection_devices.retain(|d| d.is_alive());
            composer.data_sources.retain(|_, (s, _)| s.is_alive());
            composer
                .primary_selection_sources
                .retain(|_, (s, _)| s.is_alive());

            if let Some(grab) = composer.pointer_grab.clone() {
                if !grab.is_alive() {
                    composer.cleanup_surface(&grab.id(), &dh);
                }
            }

            composer
                .subsurfaces
                .retain(|s| s.surface.is_alive() && s.parent.is_alive());

            let mut final_draw_list = composer.styler.generate_draw_list(
                &composer.subsurfaces,
                &composer.surface_textures,
                &composer.viewports,
                &composer.surface_to_viewport,
                &composer.surface_opaque_region,
                composer.wm.as_ref(),
                composer.mode.size(),
            );

            // Draw IME Popups
            let ime_surfaces = composer.get_input_popup_surfaces();
            for (surface, x, y) in ime_surfaces {
                composer.styler.draw_surface_tree(
                    &surface,
                    x,
                    y,
                    &composer.subsurfaces,
                    &composer.surface_textures,
                    &composer.viewports,
                    &composer.surface_to_viewport,
                    &mut final_draw_list,
                );
            }

            if composer.pointer_lock.is_none() {
                if let Some((cursor_surf, hot_x, hot_y)) = &composer.cursor_surface {
                    if let Some(tex) = composer.surface_textures.get(&cursor_surf.id()) {
                        final_draw_list.push(DrawCommand::Texture(RenderQuad {
                            set: tex.set,
                            x: (composer.cursor_pos.0 as f32 - *hot_x as f32).round(),
                            y: (composer.cursor_pos.1 as f32 - *hot_y as f32).round(),
                            w: tex.w / tex.scale,
                            h: tex.h / tex.scale,
                            src_x: 0.0,
                            src_y: 0.0,
                            src_w: 1.0,
                            src_h: 1.0,
                            border_radius: 0.0,
                            alpha: 1.0,
                        }));
                    }
                } else if let Some(shape) = composer.cursor_shape {
                    composer.load_cursor_shape(shape);

                    let now_ms = pattern::utils::time::gettime();

                    if let Some(frame) = composer.cursor_manager.get_frame(shape, now_ms) {
                        if let Some(anim) = composer.cursor_manager.animations.get(&shape) {
                            if anim.total_delay > 0 {
                                composer.needs_redraw = true;
                            }
                        }

                        let tex = &frame.texture;
                        let (hot_x, hot_y) = frame.hotspot;

                        final_draw_list.push(DrawCommand::Texture(RenderQuad {
                            set: tex.set,
                            x: (composer.cursor_pos.0 as f32 - hot_x).round(),
                            y: (composer.cursor_pos.1 as f32 - hot_y).round(),
                            w: tex.w / tex.scale,
                            h: tex.h / tex.scale,
                            src_x: 0.0,
                            src_y: 0.0,
                            src_w: 1.0,
                            src_h: 1.0,
                            border_radius: 0.0,
                            alpha: 1.0,
                        }));
                    }
                } else if composer.pointer_focus.is_none() {
                    let shape = wp_cursor_shape_device_v1::Shape::Default;
                    composer.load_cursor_shape(shape);

                    let now_ms = pattern::utils::time::gettime();

                    if let Some(frame) = composer.cursor_manager.get_frame(shape, now_ms) {
                        let tex = &frame.texture;
                        let (hot_x, hot_y) = frame.hotspot;

                        final_draw_list.push(DrawCommand::Texture(RenderQuad {
                            set: tex.set,
                            x: (composer.cursor_pos.0 as f32 - hot_x).round(),
                            y: (composer.cursor_pos.1 as f32 - hot_y).round(),
                            w: tex.w / tex.scale,
                            h: tex.h / tex.scale,
                            src_x: 0.0,
                            src_y: 0.0,
                            src_w: 1.0,
                            src_h: 1.0,
                            border_radius: 0.0,
                            alpha: 1.0,
                        }));
                    }
                }
            }

            // 1. we identify drawn surfaces
            let mut drawn_surface_ids = HashSet::new();
            for cmd in &final_draw_list {
                if let DrawCommand::Texture(quad) = cmd {
                    if let Some((id, _)) = composer
                        .surface_textures
                        .iter()
                        .find(|(_, t)| t.set == quad.set)
                    {
                        drawn_surface_ids.insert(id.clone());
                    }
                }
            }

            // tracing::debug!("Drawn surfaces: {:?}", drawn_surface_ids);

            let mut wait_semaphores: Vec<vk::Semaphore> = Vec::new();
            let mut wait_values: Vec<u64> = Vec::new();
            let mut signal_semaphores: Vec<vk::Semaphore> = Vec::new();
            let mut signal_values: Vec<u64> = Vec::new();

            // 2. we process Sync Points and Feedbacks
            let sync_ids: Vec<_> = composer.syncobj_state.keys().cloned().collect();
            // tracing::debug!("Sync IDs: {:?}", sync_ids);
            for id in sync_ids {
                let is_drawn = drawn_surface_ids.contains(&id);

                if is_drawn {
                    let sync_state = composer.syncobj_state.get_mut(&id).unwrap();
                    // DELAYED RELEASE: Do NOT take current_release! Only signal the old ones.
                    let acquire = sync_state.acquire_point.take();
                    let signals = std::mem::take(&mut sync_state.signal_queue);

                    if let Some((sem, point)) = acquire {
                        wait_semaphores.push(sem);
                        wait_values.push(point);
                    }

                    for (sem, point) in signals {
                        signal_semaphores.push(sem);
                        signal_values.push(point);
                    }

                    // Collecting feedbacks for presentation
                    if let Some(fbs) = composer.surface_presentation_feedbacks.remove(&id) {
                        composer.feedbacks_to_present.extend(fbs);
                    }
                } else {
                    let sync_state = composer.syncobj_state.get_mut(&id).unwrap();
                    let signals = std::mem::take(&mut sync_state.signal_queue);
                    for (sem, point) in signals {
                        signal_semaphores.push(sem);
                        signal_values.push(point);
                    }

                    // we discard feedbacks for hidden/skipped surfaces
                    if let Some(fbs) = composer.surface_presentation_feedbacks.remove(&id) {
                        for fb in fbs {
                            fb.discarded();
                        }
                    }
                }
            }

            unsafe {
                vkctx.draw_frame(
                    frame.vk_fb,
                    frame.image,
                    width as u32,
                    height as u32,
                    &final_draw_list,
                    &wait_semaphores,
                    &wait_values,
                    &signal_semaphores,
                    &signal_values,
                    frame.blur_target.as_ref(),
                    composer.styler.blur_passes(),
                );

                // NOTE: it is safe to drop semaphores here because draw_frame is synchrone.
                composer.drop_semaphores();
            }

            let mut gamma_to_apply: Option<u64> = None;
            if let Some(lut) = composer.pending_gamma.take() {
                if let Some(old_blob) = active_gamma_blob.take() {
                    let _ = card.destroy_property_blob(old_blob);
                }

                if lut.is_empty() {
                    gamma_to_apply = Some(0);
                } else {
                    match card.create_property_blob(lut.as_slice()) {
                        Ok(blob) => {
                            let blob_id = match blob {
                                drm::control::property::Value::Blob(id) => id,
                                _ => 0,
                            };
                            if blob_id > 0 {
                                active_gamma_blob = Some(blob_id);
                                gamma_to_apply = Some(blob_id);
                            }
                        }
                        Err(e) => {
                            error!("Failed to create property blob for GAMMA_LUT: {:?}", e);
                        }
                    }
                }
            }

            if initial_modeset {
                let mode_blob = card
                    .create_property_blob(&info.mode)
                    .expect("Failed to create mode blob");
                let mut req = AtomicModeReq::new();

                req.add_property(
                    info.crtc_handle,
                    info.crtc_active_prop,
                    drm::control::property::Value::UnsignedRange(1),
                );
                req.add_property(info.crtc_handle, info.crtc_mode_id_prop, mode_blob);
                req.add_property(
                    info.connector_handle,
                    info.conn_crtc_id_prop,
                    drm::control::property::Value::CRTC(Some(info.crtc_handle)),
                );
                req.add_property(
                    info.primary_plane,
                    info.plane_crtc_id_prop,
                    drm::control::property::Value::CRTC(Some(info.crtc_handle)),
                );
                req.add_property(
                    info.primary_plane,
                    info.plane_fb_id_prop,
                    drm::control::property::Value::Framebuffer(Some(frame.fb_handle)),
                );

                // Clear any hardware cursor plane atomically
                if let Some(cursor_plane) = info.cursor_plane {
                    if let Some(cursor_crtc_id_prop) = info.cursor_crtc_id_prop {
                        req.add_property(
                            cursor_plane,
                            cursor_crtc_id_prop,
                            drm::control::property::Value::CRTC(None),
                        );
                    }
                    if let Some(cursor_fb_id_prop) = info.cursor_fb_id_prop {
                        req.add_property(
                            cursor_plane,
                            cursor_fb_id_prop,
                            drm::control::property::Value::Framebuffer(None),
                        );
                    }
                }

                if let Some(gamma_val) = gamma_to_apply {
                    req.add_property(
                        info.crtc_handle,
                        info.crtc_gamma_lut_prop,
                        drm::control::property::Value::Blob(gamma_val),
                    );
                }

                card.atomic_commit(AtomicCommitFlags::ALLOW_MODESET, req)
                    .expect("Failed to set initial atomic CRTC modeset");

                initial_modeset = false;
                frame_index += 1;
                composer.needs_redraw = true;
            } else {
                let mut req = AtomicModeReq::new();
                req.add_property(
                    info.primary_plane,
                    info.plane_fb_id_prop,
                    drm::control::property::Value::Framebuffer(Some(frame.fb_handle)),
                );

                if let Some(gamma_val) = gamma_to_apply {
                    req.add_property(
                        info.crtc_handle,
                        info.crtc_gamma_lut_prop,
                        drm::control::property::Value::Blob(gamma_val),
                    );
                }

                match card.atomic_commit(
                    AtomicCommitFlags::PAGE_FLIP_EVENT | AtomicCommitFlags::NONBLOCK,
                    req,
                ) {
                    Ok(_) => {
                        waiting_for_flip = true;
                        frame_index += 1;
                    }
                    Err(e) => {
                        error!("Failed to page flip atomically: {}", e);
                        composer.needs_redraw = true; // Try again next loop
                    }
                }
            }
        }

        let _ = display.flush_clients();
    }

    info!("Tearing down swapchain");
    for frame in swapchain {
        unsafe { frame.destroy(&vkctx.device, &card) };
    }

    if let Some(blob_id) = active_gamma_blob {
        let _ = card.destroy_property_blob(blob_id);
    }

    // we drop the composer, ensuring all Arc<VulkanTextureInner> references are dropped
    drop(composer);
    drop(vkctx); // Just to be explicit about it

    info!("Engine shut down safely. Returning to the terminal.");
}
