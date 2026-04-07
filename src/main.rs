use std::{cell::RefCell, rc::Rc};

use drm::control::Device as _;
use gbm::{BufferObjectFlags, Device, Format};
use libseat::Seat;

use crate::{
    gpu::{Card, buffer::Buffer},
    input::Input,
    vulkan::VulkanContext,
};

use nix::{poll::PollTimeout, sys::epoll};

mod gpu;
mod input;
mod vulkan;

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

    // let gbm_surface: gbm::Surface<()> = gbm
    //     .create_surface(
    //         width as u32,
    //         height as u32,
    //         Format::Xrgb8888,
    //         BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
    //     )
    //     .unwrap();

    let swapchain: [gpu::buffer::Buffer<()>; 2] = [
        Buffer::new(
            gbm.create_buffer_object(
                width as u32,
                height as u32,
                Format::Xrgb8888,
                BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
            )
            .unwrap(),
        ),
        Buffer::new(
            gbm.create_buffer_object(
                width as u32,
                height as u32,
                Format::Xrgb8888,
                BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
            )
            .unwrap(),
        ),
    ];

    let mut frame_index = 0;

    println!("[pattern]: Started :)");

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

    let mut current_fb: Option<drm::control::framebuffer::Handle> = None;

    let mut waiting_for_flip = false;

    loop {
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
                    input.dispatch();
                }
                2 => {
                    shared_seat.borrow_mut().dispatch(-1).unwrap();
                }
                _ => unreachable!(),
            }
        }

        if !waiting_for_flip {
            let next_bo = &swapchain[frame_index % 2];

            unsafe {
                let (vk_image, vk_memory) =
                    vkctx.import_gbm_buffer(next_bo, width as u32, height as u32);

                let r = (input.cursor.x / width as f64) as f32;
                let g = (input.cursor.y / height as f64) as f32;
                let b = 0.5;

                vkctx.clear_image_and_wait(vk_image, r, g, b, 1.0);

                vkctx.device.destroy_image(vk_image, None);
                vkctx.device.free_memory(vk_memory, None);
            }

            let next_fb = card.add_framebuffer(next_bo, 24, 32).unwrap();

            card.page_flip(
                info.crtc_handle,
                next_fb,
                drm::control::PageFlipFlags::EVENT,
                None, // No user data for now
            )
            .expect("Failed to page flip");

            waiting_for_flip = true;

            if let Some(old_fb) = current_fb {
                card.destroy_framebuffer(old_fb).unwrap();
            }

            current_fb = Some(next_fb);
            frame_index += 1;
        }
    }
}
