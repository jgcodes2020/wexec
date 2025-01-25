use winit::platform::x11::*;

use crate::Runtime;

use super::sealed;

pub trait RuntimeExtX11: sealed::Sealed {
    fn with_x11(&mut self) -> &mut Self;
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self;
}

impl RuntimeExtX11 for Runtime {
    fn with_x11(&mut self) -> &mut Self {
        self.event_loop.with_x11();
        self
    }
    
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self {
        self.event_loop.with_any_thread(any_thread);
        self
    }
}
