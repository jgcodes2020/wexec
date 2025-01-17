use std::{cell::RefCell, future::{Future, IntoFuture}, mem, task::{Context, Poll}, thread::LocalKey};

use futures::future::LocalBoxFuture;
use slotmap::{HopSlotMap, SparseSecondaryMap};
use winit::{application::ApplicationHandler, event::StartCause, event_loop::{ActiveEventLoop, EventLoopProxy}};

use crate::task::{waker_for_task, Task, TaskID};

pub(crate) struct ExecutorHandler {
    /// Event loop proxy, used for creating wakers.
    proxy: EventLoopProxy<ExecutorEvent>,
    /// The state of the main task.
    main_task: MainTaskState,
    /// Slot map of pending futures.
    pending: HopSlotMap<TaskID, Task>,
}

impl ExecutorHandler {
    /// Polls the pending task with the given ID.
    fn poll_task_by_id(&mut self, id: TaskID, event_loop: &ActiveEventLoop) -> bool {
        todo!()
    }
}

impl ApplicationHandler<ExecutorEvent> for ExecutorHandler {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        match cause {
            StartCause::ResumeTimeReached { start, requested_resume } => todo!(),
            StartCause::WaitCancelled { start, requested_resume } => todo!(),
            StartCause::Poll => todo!(),
            StartCause::Init => todo!(),
        }
    }

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
                self.poll_task_by_id(id, event_loop);
            },
            ExecutorEvent::NewTask(task) => {
                let id = self.pending.insert(task);
                self.poll_task_by_id(id, event_loop);
            },
        }
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