use drm::buffer;
use gbm::{AsRaw, BufferObject};

pub struct Buffer<T: 'static>(BufferObject<T>);

impl<T: 'static> Buffer<T> {
    pub fn new(bo: BufferObject<T>) -> Self {
        Self(bo)
    }

    // pub fn into_raw(self) -> gbm::BufferObject<T> {
    //     self.0
    // }

    pub fn as_raw_bo(&self) -> *mut gbm_sys::gbm_bo {
        self.0.as_raw() as *mut _
    }
}

impl<T> buffer::Buffer for Buffer<T> {
    fn size(&self) -> (u32, u32) {
        (self.0.width(), self.0.height())
    }

    fn format(&self) -> buffer::DrmFourcc {
        unsafe { std::mem::transmute(self.0.format() as u32) }
    }

    fn pitch(&self) -> u32 {
        self.0.stride()
    }

    fn handle(&self) -> buffer::Handle {
        unsafe { std::mem::transmute(self.0.handle().u32_) }
    }
}
