use std::{
    any::Any, future::Future, marker::PhantomData, pin::Pin, task::{self, Poll}
};

use futures::channel::oneshot;
use winit::{event::WindowEvent, window::WindowId};

use crate::{context::with_current_rt, error::EventLoopShutdown, executor::ReturnHandle, waker::waker_id_for};

macro_rules! assert_event_loop {
    () => {
        assert!(
            crate::context::is_main_thread(),
            "event-loop futures should not be registered outside the event loop"
        );
    };
}

/// A local future that waits for the next event on a specific window.
pub struct WindowEventFuture {
    window_id: WindowId,
    handle: ReturnHandle<WindowEvent>,
}

impl WindowEventFuture {
    pub(crate) fn new(window_id: WindowId) -> Self {
        assert_event_loop!();
        Self {
            window_id,
            handle: Default::default(),
        }
    }
}

impl Future for WindowEventFuture {
    type Output = WindowEvent;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        match self.handle.take() {
            Some(value) => Poll::Ready(value),
            None => {
                let task_id = match waker_id_for(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only main-thread futures may wait for window events"),
                };
                let return_handle = self.handle.clone();
                with_current_rt(move |rt| {
                    rt.shared()
                        .arm_window_task(self.window_id, task_id, return_handle);
                });
                Poll::Pending
            }
        }
    }
}

/// A local future that waits for the event loop to resume.
pub struct ResumedFuture(());

impl ResumedFuture {
    pub(crate) fn new() -> Self {
        assert_event_loop!();
        Self(())
    }
}

impl Future for ResumedFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let resumed = with_current_rt(|rt| rt.shared().is_resumed());
        match resumed {
            true => Poll::Ready(()),
            false => {
                let task_id = match waker_id_for(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only main-thread futures may wait for suspend events"),
                };
                with_current_rt(move |rt| {
                    rt.shared().queue_resume_task(task_id);
                });
                Poll::Pending
            }
        }
    }
}

/// A local future that waits for the event loop to suspend.
pub struct SuspendedFuture(());

impl SuspendedFuture {
    pub(crate) fn new() -> Self {
        assert_event_loop!();
        Self(())
    }
}

impl Future for SuspendedFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let resumed = with_current_rt(|rt| rt.shared().is_resumed());
        match resumed {
            false => Poll::Ready(()),
            true => {
                let task_id = match waker_id_for(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only main-thread futures may wait for resume events"),
                };
                with_current_rt(move |rt| {
                    rt.shared().queue_suspend_task(task_id);
                });
                Poll::Pending
            }
        }
    }
}


/// A non-[`Send`] handle to a future executing on the main thread.
/// 
/// Dropping this handle does not prevent the completion of the future.
pub struct JoinHandle<T: 'static> {
    result: oneshot::Receiver<Box<dyn Any>>,
    _marker: PhantomData<T>,
}

impl<T: 'static> JoinHandle<T> {
    pub(crate) fn new(result: oneshot::Receiver<Box<dyn Any>>) -> Self {
        Self {
            result,
            _marker: PhantomData,
        }
    }

    fn result(self: Pin<&mut Self>) -> Pin<&mut oneshot::Receiver<Box<dyn Any>>> {
        unsafe { self.map_unchecked_mut(|val| &mut val.result) }
    }
}

impl<T: 'static> Future for JoinHandle<T> {
    type Output = Result<T, EventLoopShutdown>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        self.result().poll(cx).map(|value| match value {
            Ok(value) => Ok(*value.downcast::<T>().unwrap()),
            Err(_) => Err(EventLoopShutdown),
        })
    }
}

/// A [`Send`] handle to a future executing on the main thread. This itself
/// is a future so it can be polled from other executors. To wait on this handle
/// synchronously, you may use a crate like `pollster`.
/// 
/// Dropping this handle does not prevent the completion of the future.
pub struct SendJoinHandle<T: Send + 'static> {
    result: oneshot::Receiver<Box<dyn Any + Send>>,
    _marker: PhantomData<T>,
}

impl<T: Send + 'static> SendJoinHandle<T> {
    pub(crate) fn new(result: oneshot::Receiver<Box<dyn Any + Send>>) -> Self {
        Self {
            result,
            _marker: PhantomData,
        }
    }

    fn result(self: Pin<&mut Self>) -> Pin<&mut oneshot::Receiver<Box<dyn Any + Send>>> {
        unsafe { self.map_unchecked_mut(|val| &mut val.result) }
    }
}

impl<T: Send + 'static> Future for SendJoinHandle<T> {
    type Output = Result<T, EventLoopShutdown>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        self.result().poll(cx).map(|value| match value {
            Ok(value) => Ok(*value.downcast::<T>().unwrap()),
            Err(_) => Err(EventLoopShutdown),
        })
    }
}
