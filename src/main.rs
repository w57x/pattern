use std::{cell::RefCell, rc::Rc};

use drm::control::Device as _;
use gbm::{BufferObjectFlags, Device, Format};
use libseat::Seat;
use nix::{poll::PollTimeout, sys::epoll};
use pattern::utils;
use wayland_server::Display;

use crate::{
    gpu::{Card, buffer::Buffer},
    input::Input,
    vulkan::{VulkanContext, frame::VulkanFrame},
};

mod gpu;
mod input;
mod vulkan;

pub struct ServerState;

fn main() {
    let seat = Seat::open(|seat, event| match event {
        libseat::SeatEvent::Enable => println!("[seat]: Acquired DRM Master!"),
        libseat::SeatEvent::Disable => {
            println!("[seat]: Lost DRM Master! (User switched TTY)");
            seat.disable().unwrap();
        }
    })
    .expect("Failed to open libseat. Is seatd or systemd-logind running?");

    let shared_seat = Rc::new(RefCell::new(seat));

    let card = Card::open(None, shared_seat.clone());
    println!("[info]: {card}");
    println!("[info]: {:?}", card.get_driver().unwrap());

    let info = card.fetch_gpu_info();

    let gbm = Device::new(&card).expect("Failed to create GBM device");
    let (width, height) = info.mode.size();

    let epoll = epoll::Epoll::new(epoll::EpollCreateFlags::empty()).unwrap();

    let drm_event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 0);

    let mut input = Input::new(shared_seat.clone(), width as f64, height as f64);
    let input_event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 1);

    let seat_event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 2);

    epoll.add(&card, drm_event).unwrap();
    epoll.add(&input.context, input_event).unwrap();
    epoll
        .add(shared_seat.borrow_mut().get_fd().unwrap(), seat_event)
        .unwrap();

    println!("[pattern]: Booting Vulkan Context...");
    let vkctx = VulkanContext::new();
    println!("[pattern]: Vulkan Ready. Entering the void.");

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

    println!("[pattern]: Started :)");

    while running {
        let timeout = if waiting_for_flip {
            PollTimeout::NONE
        } else {
            PollTimeout::ZERO
        };

        let mut events = [epoll::EpollEvent::empty(); 3];
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
                    if input.dispatch() {
                        running = false;
                    }
                }
                2 => {
                    shared_seat.borrow_mut().dispatch(-1).unwrap();
                }
                _ => unreachable!(),
            }
        }

        if !waiting_for_flip {
            let frame = &swapchain[frame_index % 2];

            unsafe {
                vkctx.draw_frame(
                    frame.vk_fb,
                    desc_set,
                    width as u32,
                    height as u32,
                    input.cursor.x as f32 - hot_x,
                    input.cursor.y as f32 - hot_y,
                    cur_w,
                    cur_h,
                );
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
    }

    println!("[pattern]: Tearing down swapchain...");
    for frame in swapchain {
        unsafe { frame.destroy(&vkctx.device, &card) };
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
        vkctx
            .device
            .destroy_pipeline_layout(vkctx.pipeline_layout, None);
        vkctx.device.destroy_render_pass(vkctx.render_pass, None);

        vkctx.device.destroy_fence(vkctx.fence, None);
        vkctx.device.destroy_command_pool(vkctx.command_pool, None);
        vkctx.device.destroy_device(None);
        vkctx.instance.destroy_instance(None);
    }

    println!("[pattern]: Engine shut down safely. Returning to the terminal.");
}
