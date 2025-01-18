use std::future::{Future, IntoFuture};

use context::with_current_rt;
use executor::{Executor, ExecutorEvent};
use winit::{
    error::{EventLoopError, OsError},
    event_loop::{EventLoop, EventLoopBuilder}, window::{Window, WindowAttributes, WindowId},
};

mod context;
mod executor;
pub mod future;
pub mod reexports;
mod waker;


pub struct Runtime {
    event_loop: EventLoopBuilder<ExecutorEvent>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            event_loop: EventLoop::with_user_event(),
        }
    }

    pub fn start_with<IntoFut, Fut>(mut self, future_src: IntoFut) -> !
    where
        IntoFut: IntoFuture<Output = (), IntoFuture = Fut> + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let event_loop = self
            .event_loop
            .build()
            .expect("Failed to construct event loop");
        let mut executor = Executor::new(&event_loop, future_src);

        match event_loop.run_app(&mut executor) {
            Ok(_) => std::process::exit(0),
            Err(EventLoopError::ExitFailure(code)) => std::process::exit(code),
            Err(err) => {
                panic!("Fatal event loop error: {}", err)
            }
        }
    }
}

pub fn resumed() -> future::ResumedFuture {
    future::ResumedFuture::new()
}

pub fn suspended() -> future::SuspendedFuture {
    future::SuspendedFuture::new()
}

pub fn create_window(attrs: WindowAttributes) -> Result<Window, OsError> {
    with_current_rt(|rt| {
        rt.event_loop().create_window(attrs)
    })
}

pub fn window_event(window_id: WindowId) -> future::WindowEventFuture {
    future::WindowEventFuture::new(window_id)
}