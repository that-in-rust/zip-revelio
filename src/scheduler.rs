use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use futures::{future::BoxFuture, stream::FuturesUnordered, Stream, StreamExt};
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::{
    error::ZipError,
    stats::Stats,
    Result,
};

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Scheduled task with priority and timing information
#[derive(Debug)]
struct ScheduledTask {
    /// Task priority
    priority: Priority,
    /// Task creation time
    created_at: Instant,
    /// Task deadline
    deadline: Option<Instant>,
    /// Task future
    future: BoxFuture<'static, Result<()>>,
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.created_at == other.created_at
    }
}

impl Eq for ScheduledTask {}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // First compare by priority (higher is greater)
        let priority_ord = self.priority.cmp(&other.priority);
        if priority_ord != Ordering::Equal {
            return priority_ord;
        }

        // Then by deadline if both have one
        if let (Some(self_deadline), Some(other_deadline)) = (self.deadline, other.deadline) {
            let deadline_ord = self_deadline.cmp(&other_deadline);
            if deadline_ord != Ordering::Equal {
                return deadline_ord.reverse(); // Earlier deadline is higher priority
            }
        }

        // Finally by creation time
        self.created_at.cmp(&other.created_at).reverse()
    }
}

/// Task scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum concurrent tasks
    pub max_concurrent: usize,
    /// Default task timeout
    pub default_timeout: Duration,
    /// Task queue capacity
    pub queue_capacity: usize,
    /// Whether to enable task preemption
    pub enable_preemption: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: num_cpus::get(),
            default_timeout: Duration::from_secs(60),
            queue_capacity: 10_000,
            enable_preemption: true,
        }
    }
}

/// Task scheduler for managing concurrent tasks
pub struct Scheduler {
    /// Task queue
    queue: Arc<Mutex<BinaryHeap<ScheduledTask>>>,
    /// Currently running tasks
    running: Arc<Mutex<FuturesUnordered<BoxFuture<'static, Result<()>>>>>,
    /// Statistics collector
    stats: Arc<Stats>,
    /// Configuration
    config: SchedulerConfig,
    /// Task completion channel
    completion_tx: mpsc::Sender<Result<()>>,
    completion_rx: mpsc::Receiver<Result<()>>,
}

impl Scheduler {
    /// Creates a new scheduler
    pub fn new(stats: Arc<Stats>, config: SchedulerConfig) -> Self {
        let (completion_tx, completion_rx) = mpsc::channel(config.queue_capacity);
        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            running: Arc::new(Mutex::new(FuturesUnordered::new())),
            stats,
            config,
            completion_tx,
            completion_rx,
        }
    }

    /// Schedules a task with the given priority
    pub async fn schedule<F>(&self, priority: Priority, future: F) -> Result<()>
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        let task = ScheduledTask {
            priority,
            created_at: Instant::now(),
            deadline: Some(Instant::now() + self.config.default_timeout),
            future: Box::pin(future),
        };

        self.queue.lock().push(task);
        self.try_schedule_tasks().await
    }

    /// Schedules a task with deadline
    pub async fn schedule_with_deadline<F>(
        &self,
        priority: Priority,
        deadline: Instant,
        future: F,
    ) -> Result<()>
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        let task = ScheduledTask {
            priority,
            created_at: Instant::now(),
            deadline: Some(deadline),
            future: Box::pin(future),
        };

        self.queue.lock().push(task);
        self.try_schedule_tasks().await
    }

    /// Attempts to schedule waiting tasks
    async fn try_schedule_tasks(&self) -> Result<()> {
        let mut running = self.running.lock();
        
        while running.len() < self.config.max_concurrent {
            let next_task = self.queue.lock().pop();
            
            if let Some(task) = next_task {
                let completion_tx = self.completion_tx.clone();
                let future = async move {
                    let result = task.future.await;
                    completion_tx.send(result).await.ok();
                    Ok(())
                };
                
                running.push(Box::pin(future));
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Runs the scheduler until all tasks complete
    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(result) = self.completion_rx.recv() => {
                    if let Err(e) = result {
                        self.stats.record_error(e);
                    }
                    self.try_schedule_tasks().await?;
                }
                
                Some(result) = self.running.lock().next() => {
                    if let Err(e) = result {
                        self.stats.record_error(e);
                    }
                    self.try_schedule_tasks().await?;
                }
                
                else => break,
            }
        }

        Ok(())
    }

    /// Gets the number of queued tasks
    pub fn queued_tasks(&self) -> usize {
        self.queue.lock().len()
    }

    /// Gets the number of running tasks
    pub fn running_tasks(&self) -> usize {
        self.running.lock().len()
    }

    /// Checks if the scheduler is idle
    pub fn is_idle(&self) -> bool {
        self.queued_tasks() == 0 && self.running_tasks() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_task_scheduling() {
        let stats = Arc::new(Stats::new());
        let config = SchedulerConfig::default();
        let scheduler = Scheduler::new(Arc::clone(&stats), config);

        // Schedule tasks with different priorities
        scheduler
            .schedule(Priority::Low, async { 
                sleep(Duration::from_millis(50)).await;
                Ok(())
            })
            .await
            .unwrap();

        scheduler
            .schedule(Priority::High, async {
                sleep(Duration::from_millis(10)).await;
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(scheduler.queued_tasks(), 2);
    }

    #[tokio::test]
    async fn test_task_execution_order() {
        let stats = Arc::new(Stats::new());
        let config = SchedulerConfig::default();
        let mut scheduler = Scheduler::new(Arc::clone(&stats), config);
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        // Schedule tasks with different priorities
        for priority in [Priority::Low, Priority::High, Priority::Normal] {
            let execution_order = Arc::clone(&execution_order);
            scheduler
                .schedule(priority, async move {
                    execution_order.lock().push(priority);
                    Ok(())
                })
                .await
                .unwrap();
        }

        scheduler.run().await.unwrap();

        let order = execution_order.lock().clone();
        assert_eq!(order.len(), 3);
        assert!(order[0] == Priority::High); // High priority should execute first
    }

    #[tokio::test]
    async fn test_task_deadline() {
        let stats = Arc::new(Stats::new());
        let config = SchedulerConfig::default();
        let mut scheduler = Scheduler::new(Arc::clone(&stats), config);

        // Schedule task with deadline
        scheduler
            .schedule_with_deadline(
                Priority::Normal,
                Instant::now() + Duration::from_millis(100),
                async {
                    sleep(Duration::from_millis(50)).await;
                    Ok(())
                },
            )
            .await
            .unwrap();

        scheduler.run().await.unwrap();
        assert!(scheduler.is_idle());
    }

    #[tokio::test]
    async fn test_error_handling() {
        let stats = Arc::new(Stats::new());
        let config = SchedulerConfig::default();
        let mut scheduler = Scheduler::new(Arc::clone(&stats), config);

        // Schedule failing task
        scheduler
            .schedule(Priority::Normal, async {
                Err(ZipError::TaskError("Test error".into()))
            })
            .await
            .unwrap();

        scheduler.run().await.unwrap();
        assert!(!stats.errors().is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_execution() {
        let stats = Arc::new(Stats::new());
        let config = SchedulerConfig {
            max_concurrent: 2,
            ..Default::default()
        };
        let mut scheduler = Scheduler::new(Arc::clone(&stats), config);
        let start = Instant::now();

        // Schedule multiple tasks
        for _ in 0..4 {
            scheduler
                .schedule(Priority::Normal, async {
                    sleep(Duration::from_millis(100)).await;
                    Ok(())
                })
                .await
                .unwrap();
        }

        scheduler.run().await.unwrap();
        
        // Should take around 200ms (2 batches of 2 concurrent tasks)
        assert!(start.elapsed() >= Duration::from_millis(200));
        assert!(start.elapsed() < Duration::from_millis(400));
    }
}
