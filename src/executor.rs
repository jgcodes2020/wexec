use std::{
    collections::{HashMap, VecDeque},
    future::{Future, IntoFuture},
    mem::{self, ManuallyDrop},
    task::{Context, Poll},
};

use futures::{FutureExt as _, future::LocalBoxFuture};
use slotmap::HopSlotMap;
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

use crate::task::{id_waker, Task, TaskId};

pub(crate) struct Executor {
    /// Event loop proxy, used for creating wakers.
    proxy: EventLoopProxy<ExecutorEvent>,
    /// The state of the main task.
    main_task: MainTask,
    /// Slot map of pending futures.
    pending: HopSlotMap<TaskId, Task>,
    /// List of futures pending on resume.
    resume_pending: VecDeque<TaskId>,
    /// List of futures pending on a window event.
    window_pending: HashMap<WindowId, VecDeque<TaskId>>,
}

impl Executor {
    pub fn new<IntoFut, Fut>(event_loop: &EventLoop<ExecutorEvent>, future_src: IntoFut) -> Self
    where
        IntoFut: IntoFuture<Output = (), IntoFuture = Fut> + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        Self {
            proxy: event_loop.create_proxy(),
            main_task: MainTask::from_into_future(future_src),
            pending: HopSlotMap::with_key(),
            resume_pending: VecDeque::new(),
            window_pending: HashMap::new(),
        }
    }

    /// Gets the main future, queuing it if it hasn't already
    /// been queued.
    fn main_id(&mut self) -> TaskId {
        let task_gen = match &mut self.main_task {
            MainTask::Ready(md_task_gen) => {
                // SAFETY: we'll be overriding this value later.
                unsafe { ManuallyDrop::take(md_task_gen) }
            }
            MainTask::Running(id) => return *id,
        };
        let main_id = self.pending.insert(Task { future: task_gen() });
        self.main_task = MainTask::Running(main_id);
        main_id
    }

    /// Polls the task with the given ID.
    fn poll_task(&mut self, id: TaskId, event_loop: &ActiveEventLoop) {
        let waker = id_waker(&self.proxy, id);
        let mut context = Context::from_waker(&waker);

        match self.pending[id].poll(&mut context) {
            Poll::Ready(_) => {
                self.pending.remove(id);
                if id == self.main_id() {
                    event_loop.exit();
                }
            }
            Poll::Pending => (),
        }
    }
}

impl ApplicationHandler<ExecutorEvent> for Executor {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        match cause {
            StartCause::ResumeTimeReached {
                start,
                requested_resume,
            } => (),
            StartCause::WaitCancelled {
                start,
                requested_resume,
            } => (),
            StartCause::Poll => (),
            StartCause::Init => {
                // start and poll the main task
                let id = self.main_id();
                self.poll_task(id, event_loop);
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // separate new pending futures into their own queue; this ensures
        // that the same future cannot be polled more than once
        let pending = mem::replace(&mut self.resume_pending, VecDeque::new());
        for id in pending {
            self.poll_task(id, event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // separate new pending futures into their own queue; this ensures
        // that the same future cannot be polled more than once
        let pending = match self.window_pending.get_mut(&window_id) {
            Some(pending_for_id) => mem::replace(pending_for_id, VecDeque::new()),
            None => return,
        };
        for id in pending {
            // TODO: pass the event back to the join handle
            let _ = event;
            self.poll_task(id, event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ExecutorEvent) {
        match event {
            ExecutorEvent::Wake(id) => {
                self.poll_task(id, event_loop);
            }
            ExecutorEvent::New(task) => {
                let id = self.pending.insert(task);
                self.poll_task(id, event_loop);
            }
        }
    }
}

/// Events that may be passed to the executor.
pub(crate) enum ExecutorEvent {
    /// A Waker was triggered for this task, poll it.
    Wake(TaskId),
    /// A new task has been added to the queue.
    New(Task),
}

/// State of the main execution task.
enum MainTask {
    /// The main task is not created yet.
    Ready(ManuallyDrop<Box<dyn FnOnce() -> LocalBoxFuture<'static, ()>>>),
    /// The main task is running.
    Running(TaskId),
}

impl MainTask {
    fn from_into_future<IntoFut, Fut>(future_src: IntoFut) -> Self
    where
        IntoFut: IntoFuture<Output = (), IntoFuture = Fut> + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let contents = Box::new(move || future_src.into_future().boxed_local());
        Self::Ready(ManuallyDrop::new(contents))
    }
}
