use crate::server::{Composer, DmabufData};
use std::os::fd::AsFd;
use wayland_protocols::wp::linux_dmabuf::zv1::server::{
    zwp_linux_buffer_params_v1,
    zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    zwp_linux_dmabuf_feedback_v1,
    zwp_linux_dmabuf_feedback_v1::{TrancheFlags, ZwpLinuxDmabufFeedbackV1},
    zwp_linux_dmabuf_v1,
    zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<ZwpLinuxDmabufV1, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpLinuxDmabufV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let dmabuf = data_init.init(resource, ());

        if dmabuf.version() < 4 {
            // ARGB8888
            dmabuf.modifier(0x34325241, 0, 0);
            // XRGB8888
            dmabuf.modifier(0x34325258, 0, 0);
        }
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZwpLinuxDmabufV1,
        request: zwp_linux_dmabuf_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_linux_dmabuf_v1::Request::CreateParams { params_id } => {
                data_init.init(params_id, ());
            }
            zwp_linux_dmabuf_v1::Request::GetDefaultFeedback { id }
            | zwp_linux_dmabuf_v1::Request::GetSurfaceFeedback { id, .. } => {
                let feedback = data_init.init(id, ());

                // Send the sealed 64-byte format table (Contains 4 formats)
                feedback.format_table(state.dmabuf_table_fd.as_fd(), 64);

                // Identify the compositor's core GPU
                let dev_bytes = state.gpu_dev_t.to_ne_bytes().to_vec();
                feedback.main_device(dev_bytes.clone());

                // Define the optimal format tranche
                feedback.tranche_target_device(dev_bytes);
                feedback.tranche_flags(TrancheFlags::empty());

                // Tell Mesa to look at all 4 entries in our table
                let indices: [u8; 8] = [0, 0, 1, 0, 2, 0, 3, 0]; // Indices 0, 1, 2, 3 as u16 (LE)

                feedback.tranche_formats(indices.to_vec());
                feedback.tranche_done();

                // Conclude the transaction
                feedback.done();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for Composer {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZwpLinuxDmabufFeedbackV1,
        _request: zwp_linux_dmabuf_feedback_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for Composer {
    fn request(
        state: &mut Self,
        client: &wayland_server::Client,
        resource: &ZwpLinuxBufferParamsV1,
        request: zwp_linux_buffer_params_v1::Request,
        _data: &(),
        dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_linux_buffer_params_v1::Request::Add {
                fd,
                plane_idx,
                offset,
                stride,
                modifier_hi,
                modifier_lo,
                ..
            } if plane_idx == 0 => {
                // The client gave us the raw GPU File Descriptor
                let modifier = ((modifier_hi as u64) << 32) | (modifier_lo as u64);

                // Store it temporarily in our pending map
                state.pending_dmabufs.insert(
                    resource.id(),
                    DmabufData {
                        fd,
                        width: 0,
                        height: 0, // Set in CreateImmed
                        offset,
                        stride,
                        format: 0,
                        modifier,
                    },
                );
            }
            zwp_linux_buffer_params_v1::Request::Create {
                width,
                height,
                format,
                ..
            } => {
                if let Some(mut data) = state.pending_dmabufs.remove(&resource.id()) {
                    data.width = width as u32;
                    data.height = height as u32;
                    data.format = format;

                    let wl_buffer = client.create_resource::<wayland_server::protocol::wl_buffer::WlBuffer, (), Composer>(
                        dhandle,
                        resource.version(),
                        (),
                    ).expect("Failed to create wl_buffer resource");

                    resource.created(&wl_buffer);
                    state.dmabuffers.insert(wl_buffer.id(), data);
                } else {
                    resource.failed();
                }
            }

            zwp_linux_buffer_params_v1::Request::CreateImmed {
                buffer_id,
                width,
                height,
                format,
                ..
            } => {
                // The client finished defining the buffer. Give them a WlBuffer
                let wl_buffer = data_init.init(buffer_id, ());

                if let Some(mut data) = state.pending_dmabufs.remove(&resource.id()) {
                    data.width = width as u32;
                    data.height = height as u32;
                    data.format = format;
                    state.dmabuffers.insert(wl_buffer.id(), data);
                }
            }
            zwp_linux_buffer_params_v1::Request::Destroy => {
                state.pending_dmabufs.remove(&resource.id());
            }
            _ => {}
        }
    }
}
