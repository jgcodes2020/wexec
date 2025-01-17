use std::{
    future::Future,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use futures::future::LocalBoxFuture;
use winit::event_loop::EventLoopProxy;

use crate::executor::ExecutorEvent;

slotmap::new_key_type! {
    pub(crate) struct TaskId;
}

pub(crate) struct Task {
    pub(crate) future: LocalBoxFuture<'static, ()>,
}

impl Task {
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }
}

/// Creates a waker for the given task.
pub(crate) fn id_waker(proxy: &EventLoopProxy<ExecutorEvent>, id: TaskId) -> Waker {
    let data = Box::new(EventLoopWakerData {
        proxy: proxy.clone(),
        id,
    });
    unsafe { Waker::new(Box::into_raw(data) as *const (), &VTABLE) }
}

#[derive(Clone)]
struct EventLoopWakerData {
    proxy: EventLoopProxy<ExecutorEvent>,
    id: TaskId,
}

unsafe fn elw_clone(this: *const ()) -> RawWaker {
    // SAFETY: `this` will always be a valid pointer to EventLoopWakerData.
    let this = &mut *(this as *mut EventLoopWakerData);

    let cloned = Box::new(this.clone());
    RawWaker::new(Box::into_raw(cloned) as *const (), &VTABLE)
}

unsafe fn elw_wake(this: *const ()) {
    elw_wake_by_ref(this);
    elw_drop(this);
}

unsafe fn elw_wake_by_ref(this: *const ()) {
    // SAFETY: `this` will always be a valid pointer to EventLoopWakerData.
    let this = &mut *(this as *mut EventLoopWakerData);
    let _ = this.proxy.send_event(ExecutorEvent::Wake(this.id));
}

unsafe fn elw_drop(this: *const ()) {
    // SAFETY: `this` will always be a valid pointer to EventLoopWakerData.
    drop(Box::from_raw(this as *mut EventLoopWakerData));
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(elw_clone, elw_wake, elw_wake_by_ref, elw_drop);
