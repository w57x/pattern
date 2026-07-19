use wayland_protocols::wp::pointer_gestures::zv1::server::{
    zwp_pointer_gesture_hold_v1, zwp_pointer_gesture_pinch_v1, zwp_pointer_gesture_swipe_v1,
    zwp_pointer_gestures_v1,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<zwp_pointer_gestures_v1::ZwpPointerGesturesV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
        request: zwp_pointer_gestures_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwp_pointer_gestures_v1::Request::GetSwipeGesture { id, pointer } => {
                let swipe = data_init.init(id, ClientState);
                state
                    .swipe_gestures
                    .entry(pointer.id())
                    .or_default()
                    .push(swipe);
            }
            zwp_pointer_gestures_v1::Request::GetPinchGesture { id, pointer } => {
                let pinch = data_init.init(id, ClientState);
                state
                    .pinch_gestures
                    .entry(pointer.id())
                    .or_default()
                    .push(pinch);
            }
            zwp_pointer_gestures_v1::Request::GetHoldGesture { id, pointer } => {
                let hold = data_init.init(id, ClientState);
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

impl Dispatch<zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
        request: zwp_pointer_gesture_swipe_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let zwp_pointer_gesture_swipe_v1::Request::Destroy = request {}
    }
}

impl Dispatch<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
        request: zwp_pointer_gesture_pinch_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let zwp_pointer_gesture_pinch_v1::Request::Destroy = request {}
    }
}

impl Dispatch<zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
        request: zwp_pointer_gesture_hold_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let zwp_pointer_gesture_hold_v1::Request::Destroy = request {}
    }
}
