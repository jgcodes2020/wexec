use std::{future::Future, pin::Pin, task::{self, Poll}};

use winit::{event::WindowEvent, window::WindowId};

use crate::{context::with_current_rt, executor::{CopyReturnHandle, ReturnHandle}, waker::extract_waker_id};

pub struct WindowEventFuture {
    window_id: WindowId,
    handle: ReturnHandle<WindowEvent>,
}

impl Future for WindowEventFuture {
    type Output = WindowEvent;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        match self.handle.take() {
            Some(value) => Poll::Ready(value),
            None => {
                let task_id = match extract_waker_id(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only `wexec` futures may wait for window events"),
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

pub struct ResumedFuture {
    handle: CopyReturnHandle<()>,
}

impl Future for ResumedFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        match self.handle.take() {
            Some(_) => Poll::Ready(()),
            None => {
                let task_id = match extract_waker_id(cx.waker()) {
                    Some(id) => id,
                    None => panic!("Only `wexec` futures may wait for window events"),
                };
                let return_handle = self.handle.clone();
                with_current_rt(move |rt| {
                    rt.queues().queue_resume_task(task_id, return_handle);
                });
                Poll::Pending
            },
        }
    }
}