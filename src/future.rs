use std::{cell::{Cell, RefCell}, future::Future, pin::Pin, rc::Rc, task::{self, Poll}};

use winit::{event::WindowEvent, window::WindowId};

use crate::{context::{self, with_current_rt}, executor::{CopyReturnHandle, ReturnHandle}, waker::extract_waker_id};

pub struct WindowEventFuture {
    window_id: WindowId,
    handle: ReturnHandle<WindowEvent>,
}

impl WindowEventFuture {
    pub fn new(window_id: WindowId) -> Self {
        Self { window_id, handle: Default::default() }
    }
}

impl Future for WindowEventFuture {
    type Output = WindowEvent;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        match self.handle.take() {
            Some(value) => Poll::Ready(value),
            None => {
                let task_id = match extract_waker_id(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only main-thread futures may wait for window events"),
                };
                let return_handle = self.handle.clone();
                with_current_rt(move |rt| {
                    rt.queues().arm_window_task(self.window_id, task_id, return_handle);
                });
                Poll::Pending
            },
        }
    }
}

pub struct ResumedFuture(());

impl ResumedFuture {
    #[must_use = "This future should be `.await`ed"]
    pub fn new() -> Self {
        debug_assert!(context::is_main_thread(), "resumed() should only be called from the main thread");
        Self(())
    }
}

impl Future for ResumedFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let resumed = with_current_rt(|rt| {
            rt.queues().is_resumed()
        });
        match resumed {
            true => Poll::Ready(()),
            false => {
                let task_id = match extract_waker_id(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only main-thread futures may wait for suspend events"),
                };
                with_current_rt(move |rt| {
                    rt.queues().queue_resume_task(task_id);
                });
                Poll::Pending
            },
        }
    }
}

pub struct SuspendedFuture(());

impl SuspendedFuture {
    #[must_use = "This future should be `.await`ed"]
    pub fn new() -> Self {
        debug_assert!(context::is_main_thread(), "resumed() should only be called from the main thread");
        Self(())
    }
}

impl Future for SuspendedFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let resumed = with_current_rt(|rt| {
            rt.queues().is_resumed()
        });
        match resumed {
            false => Poll::Ready(()),
            true => {
                let task_id = match extract_waker_id(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only main-thread futures may wait for resume events"),
                };
                with_current_rt(move |rt| {
                    rt.queues().queue_suspend_task(task_id);
                });
                Poll::Pending
            },
        }
    }
}