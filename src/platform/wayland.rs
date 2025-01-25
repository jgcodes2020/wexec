use winit::platform::wayland::*;

use crate::Runtime;

use super::sealed;

pub trait RuntimeExtWayland: sealed::Sealed {
    fn with_wayland(&mut self) -> &mut Self;
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self;
}

impl RuntimeExtWayland for Runtime {
    fn with_wayland(&mut self) -> &mut Self {
        self.event_loop.with_wayland();
        self
    }
    
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self {
        self.event_loop.with_any_thread(any_thread);
        self
    }
}
