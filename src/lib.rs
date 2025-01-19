use std::{
    any::Any,
    future::{Future, IntoFuture},
};

use context::with_current_rt;
use executor::{Executor, ExecutorEvent};
use futures::{channel::oneshot, FutureExt as _};
use winit::{
    error::{EventLoopError, OsError},
    event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

mod context;
mod executor;
mod waker;

pub mod future;
pub mod reexports;
pub mod task;
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
/// This function panics if called outside the main thread.
#[must_use = "This is a future and should be `.await`ed"]
pub fn resumed() -> future::ResumedFuture {
    future::ResumedFuture::new()
}

/// Waits until the event loop suspends. If the event loop is already suspends, returns immediately.
///
/// # Panics
/// This function panics if called outside the main thread.
#[must_use = "This is a future and should be `.await`ed"]
pub fn suspended() -> future::SuspendedFuture {
    future::SuspendedFuture::new()
}

/// Waits until the window specified by `window_id` receives an event.
///
/// # Panics
/// This function panics:
/// - if called outside the main thread
/// - if another future is already waiting on this window ID.
#[must_use = "This is a future and should be `.await`ed"]
pub fn window_event(window_id: WindowId) -> future::WindowEventFuture {
    future::WindowEventFuture::new(window_id)
}

/// An external handle to the `wexec`/`winit` runtime.
/// This handle may be cloned freely.
#[derive(Debug, Clone)]
pub struct RuntimeHandle {
    proxy: EventLoopProxy<ExecutorEvent>,
}

const _: () = {
    // RuntimeHandle guarantees Send
    const fn check_send<T: Send + 'static>() {}
    check_send::<RuntimeHandle>();
};

impl RuntimeHandle {
    /// Spawns a future onto the event loop. Because [`RuntimeHandle`] is expected
    /// to be used outside the event loop, the [`IntoFuture`] passed to this function
    /// must be [`Send`].
    ///
    /// # Panics
    /// This function panics if the event loop is already shut down.
    pub fn spawn<R, IntoFut, Fut>(&self, into_future: IntoFut) -> task::SendJoinHandle<R>
    where
        R: Send + 'static,
        IntoFut: IntoFuture<Output = R, IntoFuture = Fut> + Send + 'static,
        Fut: Future<Output = R> + 'static,
    {
        let (on_ready, recv) = oneshot::channel();

        let future_src = Box::new(move || {
            into_future
                .into_future()
                .map(|x| -> Box<dyn Any + Send> { Box::new(x) })
                .boxed_local()
        });

        self.proxy
            .send_event(ExecutorEvent::NewSend {
                future_src,
                on_ready,
            })
            .expect("RuntimeHandle::spawn may only be called when the event loop is alive");

        task::SendJoinHandle::new(recv)
    }
}

/// Obtains a handle to the runtime, which may be used to spawn futures from other threads.
///
/// # Panics
/// This function panics if the event loop is already shut down.
pub fn get_handle() -> RuntimeHandle {
    with_current_rt(|rt| RuntimeHandle {
        proxy: rt.shared().clone_proxy(),
    })
}

/// Spawns a future onto the event loop. Unlike [`RuntimeHandle::spawn`], this function
/// allows non-[`Send`] futures. This function, therefore, can only be called from
/// within the main trhead.
///
/// # Panics
/// This function panics if called from outside the event loop.
pub fn spawn_local<R, IntoFut, Fut>(into_future: IntoFut) -> task::JoinHandle<R>
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
        let join_handle = task::JoinHandle::new(recv);
        rt.shared().queue_spawn_task(task);
        join_handle
    })
}
