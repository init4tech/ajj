use std::future::Future;
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::{
    sync::{CancellationToken, WaitForCancellationFuture, WaitForCancellationFutureOwned},
    task::{task_tracker::TaskTrackerWaitFuture, TaskTracker},
};

/// This is a wrapper around a [`TaskTracker`] and a [`CancellationToken`]. It
/// is used to manage a set of tasks, and to token them to shut down when the
/// set is dropped.
///
/// When a [`Handle`] is provided, tasks are spawned on that handle. Otherwise,
/// they are spawned on the current runtime.
#[derive(Debug, Clone, Default)]
pub(crate) struct TaskSet {
    tasks: TaskTracker,
    token: CancellationToken,
    handle: Option<Handle>,
}

impl From<Handle> for TaskSet {
    fn from(handle: Handle) -> Self {
        Self::with_handle(handle)
    }
}

impl TaskSet {
    /// Create a new [`TaskSet`] with a handle.
    pub(crate) fn with_handle(handle: Handle) -> Self {
        Self {
            tasks: TaskTracker::new(),
            token: CancellationToken::new(),
            handle: Some(handle),
        }
    }

    /// Get a handle to the runtime that the task set is running on.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime.
    pub(crate) fn handle(&self) -> Handle {
        self.handle
            .clone()
            .unwrap_or_else(tokio::runtime::Handle::current)
    }

    /// Cancel the token, causing all tasks to be cancelled.
    pub(crate) fn cancel(&self) {
        self.token.cancel();
        self.close();
    }

    /// Close the task tracker, allowing [`Self::wait`] futures to resolve.
    pub(crate) fn close(&self) {
        self.tasks.close();
    }

    /// Get a future that resolves when all tasks in the set are complete.
    pub(crate) fn wait(&self) -> TaskTrackerWaitFuture<'_> {
        self.tasks.wait()
    }

    pub(crate) async fn shutdown(&self) {
        self.cancel();
        self.tasks.wait().await
    }

    /// Get a future that resolves when the token is fired.
    pub(crate) fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        self.token.cancelled()
    }

    /// Get a child [`TaskSet`]. This set will be fired when the parent
    /// set is fired, or may be fired independently.
    pub(crate) fn child(&self) -> Self {
        Self {
            tasks: TaskTracker::new(),
            token: self.token.child_token(),
            handle: self.handle.clone(),
        }
    }

    /// Prepare a future to be added to the task set, by wrapping it with a
    /// cancellation token.
    fn prep_abortable_fut<F>(
        &self,
        task: F,
    ) -> impl Future<Output = Option<F::Output>> + Send + 'static
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let token = self.token.clone();
        async move {
            tokio::select! {
                _ = token.cancelled() => None,
                result = task => Some(result),
            }
        }
    }

    /// Spawn a future on the provided handle, and add it to the task set. A
    /// future spawned this way will be aborted when the [`TaskSet`] is
    /// cancelled.
    ///
    /// If the future completes before the task set is cancelled, the result
    /// will be returned. Otherwise, `None` will be returned.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime when
    /// `self.handle` is `None`.
    pub(crate) fn spawn_cancellable<F>(&self, task: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks
            .spawn_on(self.prep_abortable_fut(task), &self.handle())
    }

    /// Spawn a blocking future on the provided handle, and add it to the task
    /// set. A future spawned this way will be cancelled when the [`TaskSet`]
    /// is cancelled.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime when
    /// `self.handle` is `None`.
    pub(crate) fn spawn_blocking_cancellable<F>(&self, task: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let h = self.handle();
        let task = self.prep_abortable_fut(task);
        self.tasks
            .spawn_blocking_on(move || h.block_on(task), &self.handle())
    }

    /// Spawn a future on the provided handle, and add it to the task set. A
    /// future spawned this way will not be aborted when the [`TaskSet`] is
    /// cancelled, instead it will receive a notification via a
    /// [`CancellationToken`]. This allows the future to complete gracefully.
    /// This is useful for tasks that need to clean up resources before
    /// completing.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime when
    /// `self.handle` is `None`.
    pub(crate) fn spawn_graceful<F, Fut>(&self, task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(WaitForCancellationFutureOwned) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let token = self.token.clone().cancelled_owned();
        self.tasks.spawn_on(task(token), &self.handle())
    }

    /// Spawn a blocking future on the provided handle, and add it to the task
    /// set. A future spawned this way will not be cancelled when the
    /// [`TaskSet`] is cancelled, instead it will receive a notification via a
    /// [`CancellationToken`]. This allows the future to complete gracefully.
    /// This is useful for tasks that need to clean up resources before
    /// completing.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime when
    /// `self.handle` is `None`.
    pub(crate) fn spawn_blocking_graceful<F, Fut>(&self, task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(WaitForCancellationFutureOwned) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let h = self.handle();
        let token = self.token.clone().cancelled_owned();
        let task = task(token);
        self.tasks
            .spawn_blocking_on(move || h.block_on(task), &self.handle())
    }
}
