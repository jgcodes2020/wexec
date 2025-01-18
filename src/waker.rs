use std::task::{RawWaker, RawWakerVTable, Waker};

use winit::event_loop::EventLoopProxy;

use crate::executor::{ExecutorEvent, TaskId};


/// Creates an event-loop waker for the given task.
pub(crate) fn id_waker(proxy: &EventLoopProxy<ExecutorEvent>, id: TaskId) -> Waker {
    let data = Box::new(EventLoopWakerData {
        proxy: proxy.clone(),
        id,
    });
    unsafe { Waker::new(Box::into_raw(data) as *const (), &VTABLE) }
}

/// Extracts the [`TaskId`] from a Waker, if it is an event-loop waker.
pub(crate) fn extract_waker_id(waker: &Waker) -> Option<TaskId> {
    if waker.vtable() != &VTABLE {
        return None;
    }
    // SAFETY: this vtable will always be associated with this event waker.
    let waker_ptr = unsafe { &*(waker.data() as *const EventLoopWakerData) };
    Some(waker_ptr.id)
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