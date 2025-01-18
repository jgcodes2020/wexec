use std::{cell::RefCell, future::Future, marker::PhantomData, ptr::NonNull};

use winit::event_loop::ActiveEventLoop;

use crate::executor::{ExecutorQueues};

pub(crate) struct RuntimeContext {
    event_loop: NonNull<ActiveEventLoop>,
    queues: NonNull<ExecutorQueues>
}

impl RuntimeContext {
    pub(crate) fn event_loop(&self) -> &ActiveEventLoop {
        // SAFETY: the runtime guard will ensure this is valid.
        unsafe { self.event_loop.as_ref() }
    }

    pub(crate) fn queues(&self) -> &mut ExecutorQueues {
        // SAFETY: the runtime guard will ensure this is valid.
        unsafe { &mut *(self.queues.as_ptr()) }
    }
}

thread_local! {
    pub(crate) static CURRENT_RT: RefCell<Option<RuntimeContext>> = RefCell::new(None);
}

pub(crate) fn is_wexec_thread() -> bool {
    CURRENT_RT.with_borrow(|value| value.is_some())
}

pub(crate) fn with_current_rt<F: FnOnce(&RuntimeContext)>(f: F) {
    CURRENT_RT.with_borrow(|value| match value.as_ref() {
        Some(rt) => f(rt),
        None => panic!("Not called from the `wexec` executor"),
    });
}

pub(crate) struct RuntimeGuard<'a>(PhantomData<&'a ()>);

impl<'a> RuntimeGuard<'a> {
    pub(crate) fn with(event_loop: &'a ActiveEventLoop, queues: &'a mut ExecutorQueues) -> Self {
        CURRENT_RT.with_borrow_mut(|current_rt| {
            if current_rt.is_some() {
                panic!("Runtime context already active!");
            }

            // Since the runtime guard borrows event loop and executor,
            // we're guaranteed they have a stable address.
            *current_rt = Some(RuntimeContext {
                event_loop: NonNull::from(event_loop),
                queues: NonNull::from(queues)
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
        })
    }
}