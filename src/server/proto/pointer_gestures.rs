use crate::server::ServerState;
use wayland_protocols::wp::pointer_gestures::zv1::server::{
    zwp_pointer_gesture_hold_v1, zwp_pointer_gesture_pinch_v1, zwp_pointer_gesture_swipe_v1,
    zwp_pointer_gestures_v1,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<zwp_pointer_gestures_v1::ZwpPointerGesturesV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
        request: zwp_pointer_gestures_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_pointer_gestures_v1::Request::GetSwipeGesture { id, pointer } => {
                let swipe = data_init.init(id, ());
                state
                    .swipe_gestures
                    .entry(pointer.id())
                    .or_default()
                    .push(swipe);
            }
            zwp_pointer_gestures_v1::Request::GetPinchGesture { id, pointer } => {
                let pinch = data_init.init(id, ());
                state
                    .pinch_gestures
                    .entry(pointer.id())
                    .or_default()
                    .push(pinch);
            }
            zwp_pointer_gestures_v1::Request::GetHoldGesture { id, pointer } => {
                let hold = data_init.init(id, ());
                state
                    .hold_gestures
                    .entry(pointer.id())
                    .or_default()
                    .push(hold);
            }
            zwp_pointer_gestures_v1::Request::Release => {}
            _ => {}
        }
    }
}

impl Dispatch<zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
        request: zwp_pointer_gesture_swipe_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_pointer_gesture_swipe_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
        request: zwp_pointer_gesture_pinch_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_pointer_gesture_pinch_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
        request: zwp_pointer_gesture_hold_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_pointer_gesture_hold_v1::Request::Destroy => {}
            _ => {}
        }
    }
}
