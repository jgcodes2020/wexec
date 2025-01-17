use std::{cell::RefCell, future::{Future, IntoFuture}, mem, task::{Context, Poll}, thread::LocalKey};

use futures::future::LocalBoxFuture;
use slotmap::{HopSlotMap, SparseSecondaryMap};
use winit::{application::ApplicationHandler, event_loop::{ActiveEventLoop, EventLoopProxy}};

use crate::task::{waker_for_task, Task, TaskID};

pub(crate) struct ExecutorHandler {
    /// Event loop proxy, used for creating wakers.
    proxy: EventLoopProxy<ExecutorEvent>,
    /// Slot map of pending futures.
    pending: HopSlotMap<TaskID, Task>,
    /// Set of futures that have already been polled this iteration.
    polled: SparseSecondaryMap<TaskID, ()>,
    /// The state of the main task.
    main_task: MainTaskState,
}

impl ExecutorHandler {
    /// Polls the pending task with the given ID.
    fn poll_task_by_id(&mut self, id: TaskID, event_loop: &ActiveEventLoop) -> bool {
        let waker = waker_for_task(&self.proxy, id);
        let mut context = Context::from_waker(&waker);

        let complete = match self.pending[id].poll(&mut context) {
            Poll::Ready(_) => {
                self.pending.remove(id);
                true
            }
            Poll::Pending => false,
        };

        if let MainTaskState::Active(main_id) = &self.main_task {
            // Shut down once the main future completes.
            if complete && id == *main_id {
                event_loop.exit();
            }
        }

        complete
    }
}

impl ApplicationHandler<ExecutorEvent> for ExecutorHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let task_gen = match mem::take(&mut self.main_task) {
            MainTaskState::Waiting(task_gen) => task_gen,
            MainTaskState::Active(_) => return,
            _ => panic!("Main task should be waiting for start"),
        };

        let main_task = Task {
            future: task_gen(),
        };
        let main_id = self.pending.insert(main_task);
        if self.poll_task_by_id(main_id, event_loop) {
            return;
        }

        self.main_task = MainTaskState::Active(main_id);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        todo!()
    }

    fn user_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: ExecutorEvent,
    ) {
        match event {
            ExecutorEvent::PollTask(id) => {
                if self.polled.insert(id, ()).is_none() {
                    self.poll_task_by_id(id, &event_loop);
                }
            }
            ExecutorEvent::NewTask(task) => {
                let id = self.pending.insert(task);
                if self.polled.insert(id, ()).is_none() {
                    self.poll_task_by_id(id, &event_loop);
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.polled.clear();
    }
}

pub(crate) enum ExecutorEvent {
    PollTask(TaskID),
    NewTask(Task),
}

#[derive(Default)]
enum MainTaskState {
    #[default]
    Empty,
    Waiting(Box<dyn FnOnce() -> LocalBoxFuture<'static, ()>>),
    Active(TaskID)
}

struct ExecContext {
    active_event_loop: *const ActiveEventLoop,
    id: TaskID
}

thread_local! {
    static EXEC_CTX: RefCell<Option<ExecContext>> = RefCell::new(None);
}