use std::{
    future::Future,
    pin::Pin,
    task::{self, Poll},
};

use winit::{event::WindowEvent, window::WindowId};

use crate::{context::with_current_rt, executor::ReturnHandle, waker::waker_id_for};

macro_rules! assert_event_loop {
    () => {
        assert!(
            crate::context::is_main_thread(),
            "event-loop futures should not be registered outside the event loop"
        );
    };
}

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
