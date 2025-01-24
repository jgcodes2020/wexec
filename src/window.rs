use winit::window::Window as WinitWindow;

use crate::future::WindowEventFuture;

mod sealed {
    pub trait Sealed {}
}

/// Extensions for [winit `Window`s][winit::window::Window]. This trait is
/// sealed and cannot be implemented for custom types.
pub trait WinitWindowExt: sealed::Sealed {
    /// Registers a future to wait for the next event for this window. See
    /// [`wexec::window_event`][crate::window_event] for details.
    /// 
    /// This is equivalent to the following code:
    /// ```ignore
    /// # let window = wexec::create_window();
    /// wexec::window_event(window.id());
    /// ```
    fn next_event(&self) -> WindowEventFuture;
}

impl sealed::Sealed for WinitWindow {}
impl WinitWindowExt for WinitWindow {
    fn next_event(&self) -> WindowEventFuture {
        WindowEventFuture::new(self.id())
    }
}
