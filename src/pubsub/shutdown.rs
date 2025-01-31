use crate::TaskSet;
use tokio_util::{sync::WaitForCancellationFuture, task::task_tracker::TaskTrackerWaitFuture};

/// The shutdown signal for some server. When dropped, will cancel all tasks
/// associated with the running server. This includes all running [`Handler`]s,
/// as well as `pubsub` connection management tasks (if running).
///
/// The shutdown wraps a [`TaskTracker`] and a [`CancellationToken`], and
/// exposes methods from those APIs. Please see the documentation for those
/// types for more information.
///
/// [`TaskTracker`]: tokio_util::task::TaskTracker
/// [`CancellationToken`]: tokio_util::sync::CancellationToken
/// [`Handler`]: crate::Handler
#[derive(Debug)]
pub struct ServerShutdown {
    pub(crate) task_set: TaskSet,
}

impl From<TaskSet> for ServerShutdown {
    fn from(task_set: TaskSet) -> Self {
        Self::new(task_set)
    }
}

impl ServerShutdown {
    /// Create a new [`ServerShutdown`] with the given [`TaskSet`].
    pub(crate) const fn new(task_set: TaskSet) -> Self {
        Self { task_set }
    }

    /// Wait for the tasks spawned by the server to complete. This is a wrapper
    /// for [`TaskTracker::wait`], and allows outside code to wait for the
    /// server to signal that it has completely shut down.
    ///
    /// This future will not resolve until both of the following are true:
    /// - [`Self::close`] has been called.
    /// - All tasks spawned by the server have finished running.
    ///
    /// [`TaskTracker::wait`]: tokio_util::task::TaskTracker::wait
    pub fn wait(&self) -> TaskTrackerWaitFuture<'_> {
        self.task_set.wait()
    }

    /// Close the intenal [`TaskTracker`], cancelling all running tasks, and
    /// allowing [`Self::wait`] futures to resolve, provided all tasks are
    /// complete.
    ///
    /// This will not cancel running tasks, and will not prevent new tasks from
    /// being spawned.
    ///
    /// See [`TaskTracker::close`] for more information.
    ///
    /// [`TaskTracker`]: tokio_util::task::TaskTracker
    /// [`TaskTracker::close`]: tokio_util::task::TaskTracker::close
    pub fn close(&self) {
        self.task_set.close();
    }

    /// Check if the server's internal [`TaskTracker`] has been closed. This
    /// does not indicate that all tasks have completed, or that the server has
    /// been cancelled. See [`TaskTracker::is_closed`] for more information.
    ///
    /// [`TaskTracker`]: tokio_util::task::TaskTracker
    /// [`TaskTracker::is_closed`]: tokio_util::task::TaskTracker::is_closed
    pub fn is_closed(&self) -> bool {
        self.task_set.is_closed()
    }

    /// Issue a cancellation signal to all tasks spawned by the server. This
    /// will immediately cancel tasks spawned with the [`HandlerCtx::spawn`]
    /// family of methods, and will issue cancellation signals to tasks
    /// spawned with [`HandlerCtx::spawn_graceful`] family of methods.
    ///
    /// This will also cause new tasks spawned by the server to be immediately
    /// cancelled, or notified of cancellation.
    ///
    /// [`HandlerCtx::spawn`]: crate::HandlerCtx::spawn
    /// [`HandlerCtx::spawn_graceful`]: crate::HandlerCtx::spawn_graceful
    pub fn cancel(&self) {
        self.task_set.cancel();
    }

    /// Check if the server has been cancelled. `true` indicates that the
    /// server has been instructed to shut down, and all tasks have either been
    /// cancelled, or have received a notification that they should shut down.
    pub fn is_cancelled(&self) -> bool {
        self.task_set.is_cancelled()
    }

    /// Get a future that resolves when the server has been cancelled. This
    /// future will resolve when [`Self::cancel`] has been called, and all
    /// tasks have been issued a cancellation signal.
    pub fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        self.task_set.cancelled()
    }

    /// Shutdown the server, and wait for all tasks to complete.
    ///
    /// This is equivalent to calling [`Self::cancel`], [`Self::close`] and
    /// then awaiting [`Self::wait`].
    pub async fn shutdown(self) {
        self.task_set.cancel();
        self.close();
        self.wait().await;
    }
}

impl Drop for ServerShutdown {
    fn drop(&mut self) {
        self.task_set.cancel();
    }
}
