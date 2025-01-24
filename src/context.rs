use std::{cell::RefCell, marker::PhantomData, ptr::NonNull, sync::OnceLock};

use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};

use crate::executor::{ExecutorEvent, ExecutorShared};

pub(crate) struct RuntimeContext {
    event_loop: NonNull<ActiveEventLoop>,
    queues: NonNull<ExecutorShared>,
}

impl RuntimeContext {
    pub(crate) fn event_loop(&self) -> &ActiveEventLoop {
        // SAFETY: the runtime guard will ensure this is valid.
        unsafe { self.event_loop.as_ref() }
    }

    pub(crate) fn shared(&self) -> &mut ExecutorShared {
        // SAFETY: the runtime guard will ensure this is valid.
        unsafe { &mut *(self.queues.as_ptr()) }
    }
}

thread_local! {
    static CURRENT_RT: RefCell<Option<RuntimeContext>> = RefCell::new(None);
}

static CURRENT_PROXY: OnceLock<EventLoopProxy<ExecutorEvent>> = OnceLock::new();

pub(crate) fn is_main_thread() -> bool {
    CURRENT_RT.with_borrow(|value| value.is_some())
}

pub(crate) fn with_current_rt<R, F: FnOnce(&RuntimeContext) -> R>(f: F) -> R {
    CURRENT_RT.with_borrow(|value| match value.as_ref() {
        Some(rt) => f(rt),
        None => panic!("Not running on the main thread"),
    })
}

pub(crate) fn init_proxy(proxy: EventLoopProxy<ExecutorEvent>) {
    CURRENT_PROXY.set(proxy).expect("Unexpected event loop recreation!");
}

pub(crate) fn get_proxy() -> &'static EventLoopProxy<ExecutorEvent> {
    CURRENT_PROXY.get().expect("Main thread has not been started yet!")
}

pub(crate) struct RuntimeGuard<'a>(PhantomData<&'a ()>);

impl<'a> RuntimeGuard<'a> {
    pub(crate) fn with(event_loop: &'a ActiveEventLoop, queues: &'a mut ExecutorShared) -> Self {
        CURRENT_RT.with_borrow_mut(|current_rt| {
            if current_rt.is_some() {
                panic!("Runtime context already active!");
            }

            // Since the runtime guard borrows event loop and executor,
            // we're guaranteed they have a stable address.
            *current_rt = Some(RuntimeContext {
                event_loop: NonNull::from(event_loop),
                queues: NonNull::from(queues),
            });
        });

        Self(PhantomData)
    }
}
impl<'a> Drop for RuntimeGuard<'a> {
    fn drop(&mut self) {
        CURRENT_RT.with_borrow_mut(|current_rt| {
            if current_rt.is_none() {
                panic!("Runtime context was prematurely deleted!");
            }
            *current_rt = None;
        })
    }
}