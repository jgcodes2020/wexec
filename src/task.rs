use std::{
    any::Any,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{
    channel::oneshot::{self, Canceled},
    future::LocalBoxFuture,
    FutureExt as _,
};

slotmap::new_key_type! {
    /// Executor ID for tasks.
    pub(crate) struct TaskId;
}

pub(crate) enum Task {
    Local(LocalTask),
    Send(SendTask),
}

impl Task {
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self {
            Task::Local(local_task) => local_task.poll(cx),
            Task::Send(send_task) => send_task.poll(cx),
        }
    }
}

impl From<LocalTask> for Task {
    fn from(value: LocalTask) -> Self {
        Self::Local(value)
    }
}

impl From<SendTask> for Task {
    fn from(value: SendTask) -> Self {
        Self::Send(value)
    }
}

/// Basic task struct: a toplevel task that can be polled for results.
pub(crate) struct LocalTask {
    future: LocalBoxFuture<'static, Box<dyn Any>>,
    on_ready: Option<oneshot::Sender<Box<dyn Any>>>,
}

impl LocalTask {
    pub(crate) fn main_task(fut: LocalBoxFuture<'static, ()>) -> Self {
        Self {
            future: fut.map(|x| -> Box<dyn Any> { Box::new(x) }).boxed_local(),
            on_ready: None,
        }
    }

    pub(crate) fn new(
        future: LocalBoxFuture<'static, Box<dyn Any>>,
        on_ready: Option<oneshot::Sender<Box<dyn Any>>>,
    ) -> Self {
        Self { future, on_ready }
    }

    /// Polls the underlying future, feeding the results out where needed.
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self.future.as_mut().poll(cx) {
            Poll::Ready(result) => {
                if let Some(output) = self.on_ready.take() {
                    let _ = output.send(result);
                }
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Basic task struct: a toplevel task that can be polled for results.
pub(crate) struct SendTask {
    future: LocalBoxFuture<'static, Box<dyn Any + Send>>,
    on_ready: Option<oneshot::Sender<Box<dyn Any + Send>>>,
}

impl SendTask {
    pub(crate) fn new(
        future: LocalBoxFuture<'static, Box<dyn Any + Send>>,
        on_ready: Option<oneshot::Sender<Box<dyn Any + Send>>>,
    ) -> Self {
        Self { future, on_ready }
    }

    /// Polls the underlying future, feeding the results out where needed.
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self.future.as_mut().poll(cx) {
            Poll::Ready(result) => {
                if let Some(output) = self.on_ready.take() {
                    let _ = output.send(result);
                }
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

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
    type Output = Result<T, Canceled>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.result()
            .poll(cx)
            .map_ok(|val| *val.downcast::<T>().unwrap())
    }
}

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
    type Output = Result<T, Canceled>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.result()
            .poll(cx)
            .map_ok(|val| *val.downcast::<T>().unwrap())
    }
}
