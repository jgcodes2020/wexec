use std::{
    cell::{Cell, RefCell},
    future::{Future, IntoFuture},
    marker::PhantomData,
    ptr::NonNull,
    sync::{LazyLock, OnceLock},
};

use executor::{Executor, ExecutorEvent};
use send_wrapper::SendWrapper;
use winit::{
    error::EventLoopError,
    event_loop::{EventLoop, EventLoopBuilder},
};

mod executor;
mod task;

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