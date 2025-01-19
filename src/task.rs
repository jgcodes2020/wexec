use std::{any::Any, future::Future, marker::PhantomData, pin::Pin, task::{Context, Poll}};

use futures::{channel::oneshot::{self, Canceled}, future::{BoxFuture, LocalBoxFuture}, FutureExt as _};


slotmap::new_key_type! {
    pub(crate) struct TaskId;
}

/// Basic task struct: a toplevel task that can be polled for results.
pub(crate) struct Task {
    future: LocalBoxFuture<'static, Box<dyn Any>>,
    on_ready: Option<oneshot::Sender<Box<dyn Any>>>
}

impl Task {
    pub(crate) fn main_task(fut: LocalBoxFuture<'static, ()>) -> Self {
        Self {
            future: fut.map(|x| {
                let result: Box<dyn Any> = Box::new(x);
                result
            }).boxed_local(),
            on_ready: None
        }
    }

    pub(crate) fn local_joinable<T: 'static>(fut: LocalBoxFuture<'static, T>) -> (Task, JoinHandle<T>) {
        let (tx, rx) = oneshot::channel();

        let task = Self {
            future: fut.map(|result| {
                let result: Box<dyn Any> = Box::new(result);
                result
            }).boxed_local(),
            on_ready: Some(tx),
        };

        let join_handle = JoinHandle::<T> {
            result: rx,
            _marker: PhantomData,
        };

        (task, join_handle)
    }

    /// Polls the underlying future, feeding the results out where needed.
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self.future.as_mut().poll(cx) {
            Poll::Ready(result) => {
                if let Some(output) = self.on_ready.take() {
                    let _ = output.send(result);
                }
                Poll::Ready(())
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

pub(crate) struct JoinHandle<T: 'static> {
    result: oneshot::Receiver<Box<dyn Any>>,
    _marker: PhantomData<T>,
}

impl<T: 'static> JoinHandle<T> {
    fn result(self: Pin<&mut Self>) -> Pin<&mut oneshot::Receiver<Box<dyn Any>>> {
        unsafe { self.map_unchecked_mut(|val| &mut val.result) }
    }
}

impl<T: 'static> Future for JoinHandle<T> {
    type Output = Result<T, Canceled>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.result().poll(cx).map_ok(|val| *val.downcast::<T>().unwrap())
    }
}