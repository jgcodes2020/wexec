use winit::window::Window as WinitWindow;

use crate::future::WindowEventFuture;

mod sealed {
    pub trait Sealed {}
}

pub trait WinitWindowExt: sealed::Sealed {
    fn next_event(&self) -> WindowEventFuture;
}

impl sealed::Sealed for WinitWindow {}
impl WinitWindowExt for WinitWindow {
    fn next_event(&self) -> WindowEventFuture {
        WindowEventFuture::new(self.id())
    }
}