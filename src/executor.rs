use std::{
    any::Any, cell::{Cell, RefCell}, future::{Future, IntoFuture}, mem::{self, ManuallyDrop}, rc::Rc, task::{Context, Poll}
};

use ahash::AHashMap;
use futures::{channel::oneshot::Sender, future::LocalBoxFuture, FutureExt as _};
use slotmap::HopSlotMap;
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

use crate::{context::RuntimeGuard, task::{Task, TaskId}, waker::el_waker};

pub(crate) type ReturnHandle<T> = Rc<RefCell<Option<T>>>;
pub(crate) type CopyReturnHandle<T> = Rc<Cell<Option<T>>>;

pub(crate) struct Executor {
    /// Event loop proxy, used for waking tasks.
    proxy: EventLoopProxy<ExecutorEvent>,
    /// The state of the main task.
    main_task: MainTask,
    /// Container for all pending tasks.
    pending: HopSlotMap<TaskId, Task>,
    /// Queues that tasks may place themselves on
    /// to be woken up
    shared: ExecutorShared,
}

impl Executor {
    pub(crate) fn new<IntoFut, Fut>(
        event_loop: &EventLoop<ExecutorEvent>,
        future_src: IntoFut,
    ) -> Self
    where
        IntoFut: IntoFuture<Output = (), IntoFuture = Fut> + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        Self {
            proxy: event_loop.create_proxy(),
            main_task: MainTask::from_into_future(future_src),
            pending: HopSlotMap::with_key(),
            shared: ExecutorShared::new(),
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
        let main_id = self.pending.insert(Task::main_task(task_gen()));
        self.main_task = MainTask::Running(main_id);
        main_id
    }

    /// Polls the task with the given ID.
    fn poll_task(&mut self, id: TaskId, event_loop: &ActiveEventLoop) {
        let _rt_guard = RuntimeGuard::with(event_loop, &mut self.shared);

        let waker = el_waker(&self.proxy, id);
        let mut context = Context::from_waker(&waker);

        match self.pending[id].poll(&mut context) {
            Poll::Ready(_) => {
                self.pending.remove(id);
                // the event loop shuts down when the main task completes
                if id == self.main_task.id() {
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
            StartCause::Init => {
                // start and poll the main task
                let id = self.main_id();
                self.poll_task(id, event_loop);
            }
            _ => (),
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.shared.set_resumed(true);
        for id in self.shared.drain_resume_tasks() {
            self.poll_task(id, event_loop);
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.shared.set_resumed(false);
        for id in self.shared.drain_suspend_tasks() {
            self.poll_task(id, event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Poll the task waiting on this event
        if let Some((id, handle)) = self.shared.trip_window_task(window_id) {
            *handle.borrow_mut() = Some(event);
            self.poll_task(id, event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ExecutorEvent) {
        match event {
            ExecutorEvent::Wake(id) => {
                self.poll_task(id, event_loop);
            }
        }
    }
}

/// Shared state of the executor that is indirectly exposed to futures.
pub(crate) struct ExecutorShared {
    /// List of tasks waiting for the event loop to resume.
    pending_resume: Vec<TaskId>,
    /// List of tasks waiting for the event loop to suspend.
    pending_suspend: Vec<TaskId>,
    /// Map of all tasks waiting for a window event.
    pending_window: AHashMap<WindowId, (TaskId, ReturnHandle<WindowEvent>)>,
    /// True when the event loop resumes.
    is_resumed: bool,
}

impl ExecutorShared {
    fn new() -> Self {
        Self {
            pending_resume: Vec::with_capacity(4),
            pending_suspend: Vec::with_capacity(4),
            pending_window: AHashMap::with_capacity(4),
            is_resumed: false,
        }
    }

    pub(crate) fn is_resumed(&self) -> bool {
        self.is_resumed
    }

    fn set_resumed(&mut self, resumed: bool) {
        self.is_resumed = resumed;
    }

    /// Queues `task` to be woken up when the event loop resumes.
    pub(crate) fn queue_resume_task(&mut self, task: TaskId) {
        self.pending_resume.push(task);
    }

    /// Dequeues all tasks waiting for resume, returning them as an iterator.
    fn drain_resume_tasks(&mut self) -> impl Iterator<Item = TaskId> {
        mem::replace(&mut self.pending_resume, Vec::with_capacity(4)).into_iter()
    }

    /// Queues `task` to be woken up when the event loop suspend.
    pub(crate) fn queue_suspend_task(&mut self, task: TaskId) {
        self.pending_suspend.push(task);
    }

    /// Dequeues all tasks waiting for suspend, returning them as an iterator.
    fn drain_suspend_tasks(&mut self) -> impl Iterator<Item = TaskId> {
        mem::replace(&mut self.pending_suspend, Vec::with_capacity(4)).into_iter()
    }

    /// Arms `task` to be awoken when `window` receives an event.
    /// Only one task may await any given window's events at a time.
    pub(crate) fn arm_window_task(
        &mut self,
        window: WindowId,
        task: TaskId,
        handle: ReturnHandle<WindowEvent>,
    ) {
        let mut insert_succeeded: bool = false;
        self.pending_window.entry(window).or_insert_with(|| {
            insert_succeeded = true;
            (task, handle)
        });
        if !insert_succeeded {
            panic!("Only one task may await a window's next event at a time (window: {:?}, task: {:?})", window, task);
        }
    }

    /// Returns the task to awake for this window event, if there is one.
    fn trip_window_task(
        &mut self,
        window: WindowId,
    ) -> Option<(TaskId, ReturnHandle<WindowEvent>)> {
        self.pending_window.remove(&window)
    }
}

/// Events that may be passed to the executor.
pub(crate) enum ExecutorEvent {
    /// A Waker was triggered for this task, poll it.
    Wake(TaskId),
}

/// State of the main execution task.
enum MainTask {
    /// The main task is not created yet.
    Ready(ManuallyDrop<Box<dyn FnOnce() -> LocalBoxFuture<'static, ()>>>),
    /// The main task is running.
    Running(TaskId),
}

impl MainTask {
    /// Derives a main task from an [`IntoFuture`].
    fn from_into_future<IntoFut, Fut>(future_src: IntoFut) -> Self
    where
        IntoFut: IntoFuture<Output = (), IntoFuture = Fut> + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let contents = Box::new(move || future_src.into_future().boxed_local());
        Self::Ready(ManuallyDrop::new(contents))
    }

    fn id(&self) -> TaskId {
        if let Self::Running(task_id) = self {
            return *task_id;
        } else {
            panic!("The main task has not started");
        }
    }
}
