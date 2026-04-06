use drm::control::Device as _;
use gbm::{AsRaw as _, BufferObjectFlags, Device, Format};
use khronos_egl as egl;

use crate::{
    gpu::{Card, buffer::Buffer},
    input::Input,
};

use nix::{poll::PollTimeout, sys::epoll};

mod gpu;
mod input;

const PLATFORM_GBM_MESA: egl::Enum = 0x31D7;

fn main() {
    let card = Card::open(None);
    println!("[info]: {:?}", card.get_driver().unwrap());

    let info = card.fetch_gpu_info();

    let gbm = Device::new(&card).expect("Failed to create GBM device");
    let (width, height) = info.mode.size();

    let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required().unwrap() };
    let egl_display = unsafe {
        egl.get_platform_display(
            PLATFORM_GBM_MESA,
            gbm.as_raw() as *mut _,
            &[egl::ATTRIB_NONE],
        )
        .expect("Failed to get EGL display via GBM platform")
    };

    egl.initialize(egl_display)
        .expect("Failed to initialize EGL");

    #[rustfmt::skip]
    let config_attributes = [
        egl::RED_SIZE, 8,
        egl::GREEN_SIZE, 8,
        egl::BLUE_SIZE, 8,
        egl::ALPHA_SIZE, 8,
        egl::DEPTH_SIZE, 0,
        egl::SURFACE_TYPE, egl::WINDOW_BIT,
        egl::RENDERABLE_TYPE, egl::OPENGL_ES2_BIT,
        egl::NONE,
    ];

    let count = egl
        .matching_config_count(egl_display, &config_attributes)
        .unwrap();

    let mut configs = Vec::with_capacity(count);
    egl.choose_config(egl_display, &config_attributes, &mut configs)
        .unwrap();
    let config = configs[0];

    let context_attribs = [egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE];

    let egl_context = egl
        .create_context(egl_display, config, None, &context_attribs)
        .unwrap();

    let gbm_surface: gbm::Surface<()> = gbm
        .create_surface(
            width as u32,
            height as u32,
            Format::Xrgb8888,
            BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
        )
        .unwrap();

    let egl_surface = unsafe {
        egl.create_platform_window_surface(
            egl_display,
            config,
            gbm_surface.as_raw() as *mut _,
            &[egl::ATTRIB_NONE],
        )
        .unwrap()
    };

    egl.make_current(
        egl_display,
        Some(egl_surface),
        Some(egl_surface),
        Some(egl_context),
    )
    .unwrap();

    gl::load_with(|s| {
        egl.get_proc_address(s)
            .map(|f| f as *const _)
            .unwrap_or(std::ptr::null())
    });

    unsafe {
        gl::Viewport(0, 0, width as i32, height as i32);
    }

    println!("[pattern]: Started :)");

    let epoll = epoll::Epoll::new(epoll::EpollCreateFlags::empty()).unwrap();
    let drm_event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 0);
    epoll.add(&card, drm_event).unwrap();

    let mut current_fb: Option<drm::control::framebuffer::Handle> = None;
    let mut current_bo: Option<Buffer<()>> = None;

    let mut input = Input::new(width as f64, height as f64);

    let input_event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 1);
    epoll.add(&input.context, input_event).unwrap();

    let mut waiting_for_flip = false;

    loop {
        let timeout = if waiting_for_flip {
            PollTimeout::NONE
        } else {
            PollTimeout::ZERO
        };

        let mut events = [epoll::EpollEvent::empty(); 2];
        let num_events = epoll.wait(&mut events, timeout).unwrap();

        for i in 0..num_events {
            match events[i].data() {
                0 => {
                    let drm_events = card.receive_events().unwrap();
                    for event in drm_events {
                        if let drm::control::Event::PageFlip(_) = event {
                            waiting_for_flip = false;
                        }
                    }
                }
                1 => {
                    input.dispatch();
                }
                _ => unreachable!(),
            }
        }

        if !waiting_for_flip {
            unsafe {
                gl::ClearColor(1.0, 1.0, 1.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);

                gl::Enable(gl::SCISSOR_TEST);

                gl::Scissor(
                    input.cursor.x as i32,
                    (height as f64 - input.cursor.y - 10.0) as i32,
                    10,
                    10,
                );
                gl::ClearColor(0.0, 0.0, 0.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::Disable(gl::SCISSOR_TEST);
            }

            egl.swap_buffers(egl_display, egl_surface).unwrap();

            let next_bo = unsafe { gbm_surface.lock_front_buffer().unwrap() };
            let wrapped_next_bo = Buffer::new(next_bo);
            let next_fb = card.add_framebuffer(&wrapped_next_bo, 24, 32).unwrap();

            card.page_flip(
                info.crtc_handle,
                next_fb,
                drm::control::PageFlipFlags::EVENT,
                None, // No user data for now
            )
            .expect("Failed to page flip");

            waiting_for_flip = true;

            if let Some(old_fb) = current_fb {
                unsafe {
                    card.destroy_framebuffer(old_fb).unwrap();
                    // We release the old BO back to the pool
                    if let Some(old_bo) = current_bo {
                        gbm_sys::gbm_surface_release_buffer(
                            gbm_surface.as_raw() as *mut _,
                            old_bo.as_raw_bo(),
                        );
                    }
                }
            }

            current_fb = Some(next_fb);
            current_bo = Some(wrapped_next_bo);
        }
    }
}
