use std::{
    any::Any,
    future::{Future, IntoFuture},
};

use context::{get_proxy, with_current_rt};
use executor::{Executor, ExecutorEvent};
use futures::{channel::oneshot, FutureExt as _};
use winit::{
    error::{EventLoopError, OsError},
    event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

mod context;
mod executor;
mod task;
mod waker;

pub mod error;
pub mod future;
pub mod reexports;
pub mod window;

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

/// Creates a window. This forwards directly to [`ActiveEventLoop::create_window`][winit::event_loop::ActiveEventLoop::create_window],
///
pub fn create_window(attrs: WindowAttributes) -> Result<Window, OsError> {
    with_current_rt(|rt| rt.event_loop().create_window(attrs))
}

/// Waits until the event loop resumes. If the event loop is already resumed, returns immediately.
/// For maximum cross-platform compatibility, it is recommended to wait on [`resumed()`] before
/// creating any windows.
///
/// # Panics
/// This function panics if called outside the event loop.
#[must_use = "This is a future and should be `.await`ed"]
pub fn resumed() -> future::ResumedFuture {
    future::ResumedFuture::new()
}

/// Waits until the event loop suspends. If the event loop is already suspended, returns immediately.
///
/// # Panics
/// This function panics if called outside the event loop.
#[must_use = "This is a future and should be `.await`ed"]
pub fn suspended() -> future::SuspendedFuture {
    future::SuspendedFuture::new()
}

/// Waits until the window specified by `window_id` receives an event.
///
/// # Panics
/// This function panics:
/// - if called outside the event loop
/// - if another future is already waiting on this window ID.
#[must_use = "This is a future and should be `.await`ed"]
pub fn window_event(window_id: WindowId) -> future::WindowEventFuture {
    future::WindowEventFuture::new(window_id)
}

/// Spawns a future onto the event loop. This is the recommended way to interact with
/// winit [`Window`]s. Because this function may be called from any thread, all data
/// passing in and out of this function must be [`Send`].
pub fn spawn<R, IntoFut, Fut>(into_future: IntoFut) -> future::SendJoinHandle<R>
where
    R: Send + 'static,
    IntoFut: IntoFuture<Output = R, IntoFuture = Fut> + Send + 'static,
    Fut: Future<Output = R> + 'static,
{
    let proxy = get_proxy();
    let (on_ready, recv) = oneshot::channel();

    let future_src = Box::new(move || {
        into_future
            .into_future()
            .map(|x| -> Box<dyn Any + Send> { Box::new(x) })
            .boxed_local()
    });

    proxy
        .send_event(ExecutorEvent::NewSend {
            future_src,
            on_ready,
        })
        .expect("RuntimeHandle::spawn may only be called when the event loop is alive");

    future::SendJoinHandle::new(recv)
}

/// Spawns a future onto the event loop. Unlike [`spawn`], this function
/// allows non-[`Send`] futures, but it can only be called from within the
/// event loop.
///
/// # Panics
/// This function panics if called from outside the event loop.
pub fn spawn_local<R, IntoFut, Fut>(into_future: IntoFut) -> future::JoinHandle<R>
where
    R: 'static,
    IntoFut: IntoFuture<Output = R, IntoFuture = Fut> + 'static,
    Fut: Future<Output = R> + 'static,
{
    with_current_rt(move |rt| {
        let (on_ready, recv) = oneshot::channel();
        let task = task::LocalTask::new(
            into_future
                .into_future()
                .map(|x| -> Box<dyn Any> { Box::new(x) })
                .boxed_local(),
            Some(on_ready),
        );
        let join_handle = future::JoinHandle::new(recv);
        rt.shared().queue_spawn_task(task);
        join_handle
    })
}
