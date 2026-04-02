use anyhow::Result;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::collections::HashMap;
use uuid::Uuid;

/// Represents a message sent from a worker agent back to the coordinator.
#[derive(Debug)]
pub struct TaskNotification {
    pub task_id: String,
    pub status: String,
    pub summary: String,
    pub result: Option<String>,
}

/// The Coordinator manages multiple sub-agents (workers).
/// In the TypeScript version, this was handled implicitly by the Node.js event loop
/// firing async functions. In Rust, we use tokio::spawn to run agents in parallel
/// and an mpsc channel to receive their results asynchronously.
pub struct Coordinator {
    /// Sender channel given to new workers to report back.
    tx: mpsc::Sender<TaskNotification>,
    /// Receiver channel for the coordinator to read worker notifications.
    rx: mpsc::Receiver<TaskNotification>,
    /// Active worker tasks.
    workers: HashMap<String, JoinHandle<()>>,
}

impl Coordinator {
    pub fn new() -> Self {
        // Channel with capacity 32 for task notifications
        let (tx, rx) = mpsc::channel(32);
        Self {
            tx,
            rx,
            workers: HashMap::new(),
        }
    }

    /// Spawns a new worker agent. This maps to the `AgentTool` in TypeScript.
    pub fn spawn_worker(&mut self, prompt: String) -> String {
        let task_id = Uuid::new_v4().to_string();
        let tx_clone = self.tx.clone();
        let id_clone = task_id.clone();

        // Spawn the worker task
        let handle = tokio::spawn(async move {
            println!("[Worker {}] Started: {}", id_clone, prompt);

            // In a real implementation, this would instantiate a new `QueryEngine`
            // tailored for the worker, give it a subset of tools, and call `.submit_message()`

            // Simulate work
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            let simulated_result = format!("Simulated findings for: {}", prompt);
            println!("[Worker {}] Finished.", id_clone);

            // Send result back to coordinator
            let notification = TaskNotification {
                task_id: id_clone,
                status: "completed".to_string(),
                summary: "Worker completed successfully".to_string(),
                result: Some(simulated_result),
            };

            if let Err(e) = tx_clone.send(notification).await {
                eprintln!("Failed to send task notification: {}", e);
            }
        });

        self.workers.insert(task_id.clone(), handle);
        task_id
    }

    /// Process incoming notifications from workers.
    /// This should be integrated into the main QueryEngine loop, where it would
    /// inject the `<task-notification>` XML block back into the LLM context.
    pub async fn process_notifications(&mut self) -> Result<Vec<TaskNotification>> {
        let mut notifications = Vec::new();

        // Non-blocking try_recv to grab any pending notifications
        while let Ok(notification) = self.rx.try_recv() {
            self.workers.remove(&notification.task_id);
            notifications.push(notification);
        }

        Ok(notifications)
    }

    /// Awaits all active workers. Mostly used for shutdown.
    pub async fn wait_all(&mut self) {
        for (_, handle) in self.workers.drain() {
            let _ = handle.await;
        }
    }
}
