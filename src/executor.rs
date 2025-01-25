use std::{
    any::Any,
    cell::RefCell,
    collections::VecDeque,
    fmt::Debug,
    future::{Future, IntoFuture},
    mem::{self, ManuallyDrop},
    rc::Rc,
    task::{Context, Poll},
};

use ahash::AHashMap;
use futures::{
    channel::oneshot::{self},
    future::LocalBoxFuture,
    FutureExt as _,
};
use slotmap::HopSlotMap;
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

use crate::{
    context::{self, RuntimeGuard},
    task::{LocalTask, SendTask, Task, TaskId},
    waker::el_waker,
};

pub(crate) type ReturnHandle<T> = Rc<RefCell<Option<T>>>;
// pub(crate) type CopyReturnHandle<T> = Rc<Cell<Option<T>>>;

pub(crate) struct Executor {
    /// The state of the main task.
    main_task: MainTask,
    /// Container for all pending tasks.
    pending: HopSlotMap<TaskId, Task>,
    /// Shared state between the executor
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
        let proxy = event_loop.create_proxy();
        context::init_proxy(proxy.clone());

        Self {
            main_task: MainTask::from_into_future(future_src),
            pending: HopSlotMap::with_key(),
            shared: ExecutorShared::new(proxy),
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
        let main_id = self.pending.insert(LocalTask::main_task(task_gen()).into());
        self.main_task = MainTask::Running(main_id);
        main_id
    }

    /// Polls the task with the given ID.
    fn poll_task(&mut self, id: TaskId, event_loop: &ActiveEventLoop) {
        let waker = el_waker(&self.shared.proxy, id);
        let mut context = Context::from_waker(&waker);

        match {
            // setup runtime context and poll the task
            let _rt_guard = RuntimeGuard::with(event_loop, &mut self.shared);
            self.pending[id].poll(&mut context)
        } {
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
            ExecutorEvent::Wake { id } => {
                self.poll_task(id, event_loop);
            }
            ExecutorEvent::NewSend {
                future_src,
                on_ready,
            } => {
                let task: Task = SendTask::new(future_src(), Some(on_ready)).into();
                let id = self.pending.insert(task);
                self.poll_task(id, event_loop);
            }
            ExecutorEvent::NewLocal => {
                let task: Task = self.shared.next_spawn_task().into();
                let id = self.pending.insert(task);
                self.poll_task(id, event_loop);
            }
        }
    }
}

const BASE_CAPACITY: usize = 4;

/// Shared state of the executor that is indirectly exposed to futures.
pub(crate) struct ExecutorShared {
    /// Event loop proxy, used for waking tasks.
    proxy: EventLoopProxy<ExecutorEvent>,
    /// List of tasks waiting for the event loop to resume.
    /// These are all simultaneously dequeued once a resume occurs.
    resume_queue: Vec<TaskId>,
    /// List of tasks waiting for the event loop to suspend.
    /// These are all simultaneously dequeued once a suspend occurs.
    suspend_queue: Vec<TaskId>,
    /// Map of tasks waiting for a window event, by ID. Only one task per window.
    window_event_queue: AHashMap<WindowId, (TaskId, ReturnHandle<WindowEvent>)>,
    /// Queue of tasks spawned locally on the event queue.
    spawn_queue: VecDeque<LocalTask>,
    /// True when the event loop resumes.
    is_resumed: bool,
}

impl ExecutorShared {
    fn new(proxy: EventLoopProxy<ExecutorEvent>) -> Self {
        Self {
            proxy,
            resume_queue: Vec::with_capacity(BASE_CAPACITY),
            suspend_queue: Vec::with_capacity(BASE_CAPACITY),
            window_event_queue: AHashMap::with_capacity(BASE_CAPACITY),
            spawn_queue: VecDeque::with_capacity(BASE_CAPACITY),
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
        self.resume_queue.push(task);
    }

    /// Dequeues all tasks waiting for resume, returning them as an iterator.
    fn drain_resume_tasks(&mut self) -> impl Iterator<Item = TaskId> {
        mem::replace(&mut self.resume_queue, Vec::with_capacity(BASE_CAPACITY)).into_iter()
    }

    /// Queues `task` to be woken up when the event loop suspend.
    pub(crate) fn queue_suspend_task(&mut self, task: TaskId) {
        self.suspend_queue.push(task);
    }

    /// Dequeues all tasks waiting for suspend, returning them as an iterator.
    fn drain_suspend_tasks(&mut self) -> impl Iterator<Item = TaskId> {
        mem::replace(&mut self.suspend_queue, Vec::with_capacity(BASE_CAPACITY)).into_iter()
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
        self.window_event_queue.entry(window).or_insert_with(|| {
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
        self.window_event_queue.remove(&window)
    }

    /// Queues a local task to spawn on the event loop.
    pub(crate) fn queue_spawn_task(&mut self, task: LocalTask) {
        self.spawn_queue.push_back(task);
        self.proxy
            .send_event(ExecutorEvent::NewLocal)
            .expect("Event loop should be running");
    }

    /// Dequeues all local tasks to be spawned, returning them as an iterator.
    fn next_spawn_task(&mut self) -> LocalTask {
        self.spawn_queue.pop_front().unwrap()
    }
}

/// Events that may be passed to the executor from outside the event loop.
pub(crate) enum ExecutorEvent {
    /// A waker was tripped.
    Wake {
        /// The ID of the task that needs to be polled.
        id: TaskId,
    },
    /// A new task was submitted to the runtime.
    NewSend {
        /// A function returning the task's future.
        future_src: Box<dyn FnOnce() -> LocalBoxFuture<'static, Box<dyn Any + Send>> + Send>,
        /// A one-shot sender to return the result to.
        on_ready: oneshot::Sender<Box<dyn Any + Send>>,
    },
    /// A new local task was submitted to the runtime.
    NewLocal,
}

impl Debug for ExecutorEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wake { id } => f.debug_struct("Wake").field("id", id).finish(),
            Self::NewSend { .. } => f.debug_struct("NewSend").finish_non_exhaustive(),
            Self::NewLocal => f.write_str("NewLocal"),
        }
    }
}

const _: () = {
    // sanity check: ExecutorEvent should be Send + 'static
    const fn test<T: Send + 'static>() {}
    test::<ExecutorEvent>();
};

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
