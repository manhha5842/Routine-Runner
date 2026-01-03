//! Scheduler Runner - Background task scheduler

use crate::conditions::evaluate_conditions;
use crate::executor::{execute_task, ExecutionResult};
use crate::models::*;
use crate::scheduler::compute_next_run;
use crate::storage::Database;
use chrono::{Local, Utc};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Scheduler state
pub struct SchedulerRunner {
    db: Arc<Database>,
    paused: Arc<AtomicBool>,
    running_tasks: Arc<Mutex<HashSet<String>>>,
    max_parallel: u8,
}

impl SchedulerRunner {
    pub fn new(db: Arc<Database>, max_parallel: u8) -> Self {
        Self {
            db,
            paused: Arc::new(AtomicBool::new(false)),
            running_tasks: Arc::new(Mutex::new(HashSet::new())),
            max_parallel,
        }
    }
    
    /// Pause the scheduler
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        tracing::info!("Scheduler paused");
    }
    
    /// Resume the scheduler
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        tracing::info!("Scheduler resumed");
    }
    
    /// Check if scheduler is paused
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
    
    /// Toggle pause state
    pub fn toggle_pause(&self) -> bool {
        let was_paused = self.paused.fetch_xor(true, Ordering::SeqCst);
        let is_now_paused = !was_paused;
        tracing::info!("Scheduler {}", if is_now_paused { "paused" } else { "resumed" });
        is_now_paused
    }
    
    /// Run the scheduler loop
    pub async fn run(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            
            if self.is_paused() {
                continue;
            }
            
            if let Err(e) = self.tick().await {
                tracing::error!("Scheduler tick error: {}", e);
            }
        }
    }
    
    /// Single tick of the scheduler
    async fn tick(&self) -> Result<(), String> {
        let tasks = self.db.get_all_tasks().map_err(|e| e.to_string())?;
        let now_local = Local::now();
        let now_utc = Utc::now();
        
        for task in tasks {
            if !task.enabled {
                continue;
            }
            
            // Get task state
            let state = self.get_task_state(&task.id);
            
            // Check each trigger
            for trigger in &task.triggers {
                if let Some(next_run) = compute_next_run(trigger, now_local, &state) {
                    if next_run <= now_utc {
                        // Task is due!
                        self.execute_task_if_ready(&task, trigger, &state).await?;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get task state from database
    fn get_task_state(&self, task_id: &str) -> TaskState {
        // TODO: Actually fetch from database
        TaskState {
            task_id: task_id.to_string(),
            last_run_date_local: None,
            last_run_at_utc: None,
            last_result: None,
            last_error: None,
            next_run_at_utc: None,
        }
    }
    
    /// Execute task if all conditions are met
    async fn execute_task_if_ready(
        &self,
        task: &Task,
        trigger: &Trigger,
        _state: &TaskState,
    ) -> Result<(), String> {
        // Check if already running (singleton)
        if task.singleton {
            let running = self.running_tasks.lock().await;
            if running.contains(&task.id) {
                tracing::info!("Task {} already running (singleton), skipping", task.name);
                self.log_skip(&task, trigger, SkipReason::Singleton);
                return Ok(());
            }
        }
        
        // Check max parallel
        {
            let running = self.running_tasks.lock().await;
            if running.len() >= self.max_parallel as usize {
                tracing::info!("Max parallel tasks reached, queuing {}", task.name);
                return Ok(());
            }
        }
        
        // Check conditions
        match evaluate_conditions(&task.conditions) {
            Ok(true) => {}
            Ok(false) => {
                tracing::info!("Conditions not met for task {}", task.name);
                self.log_skip(&task, trigger, SkipReason::ConditionFail);
                return Ok(());
            }
            Err(e) => {
                tracing::error!("Error evaluating conditions: {}", e);
                return Err(e);
            }
        }
        
        // Execute!
        tracing::info!("Executing task: {}", task.name);
        
        // Mark as running
        {
            let mut running = self.running_tasks.lock().await;
            running.insert(task.id.clone());
        }
        
        // Apply start delay
        if task.start_delay_seconds > 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                task.start_delay_seconds as u64,
            ))
            .await;
        }
        
        // Run the task
        let result = execute_task(task);
        
        // Mark as not running
        {
            let mut running = self.running_tasks.lock().await;
            running.remove(&task.id);
        }
        
        // Log result
        self.log_execution(task, trigger, &result);
        
        // Update task state
        self.update_task_state(task, &result);
        
        Ok(())
    }
    
    /// Log a skipped execution
    fn log_skip(&self, task: &Task, trigger: &Trigger, reason: SkipReason) {
        let log = RunLog {
            run_id: uuid::Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            trigger_type: format!("{:?}", trigger),
            scheduled_time_utc: Some(Utc::now()),
            started_at_utc: Utc::now(),
            finished_at_utc: Some(Utc::now()),
            status: RunStatus::Skipped,
            skip_reason: Some(reason),
            exit_code: None,
            error_message: None,
            output: None,
        };
        
        if let Err(e) = self.db.insert_log(&log) {
            tracing::error!("Failed to insert log: {}", e);
        }
    }
    
    /// Log an execution result
    fn log_execution(
        &self,
        task: &Task,
        trigger: &Trigger,
        result: &Result<ExecutionResult, crate::executor::ExecutorError>,
    ) {
        let (status, error_message, exit_code, output) = match result {
            Ok(r) => {
                if r.success {
                    (RunStatus::Success, None, r.exit_code, r.output.clone())
                } else {
                    (RunStatus::Failed, r.error_message.clone(), r.exit_code, r.output.clone())
                }
            }
            Err(e) => (RunStatus::Failed, Some(e.to_string()), None, None),
        };
        
        let log = RunLog {
            run_id: uuid::Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            trigger_type: format!("{:?}", trigger),
            scheduled_time_utc: Some(Utc::now()),
            started_at_utc: Utc::now(),
            finished_at_utc: Some(Utc::now()),
            status,
            skip_reason: None,
            exit_code,
            error_message,
            output,
        };
        
        if let Err(e) = self.db.insert_log(&log) {
            tracing::error!("Failed to insert log: {}", e);
        }
    }
    
    /// Update task state after execution
    fn update_task_state(
        &self,
        task: &Task,
        result: &Result<ExecutionResult, crate::executor::ExecutorError>,
    ) {
        let now_local = Local::now();
        let last_result = match result {
            Ok(r) if r.success => RunResult::Success,
            _ => RunResult::Failed,
        };
        
        let _state = TaskState {
            task_id: task.id.clone(),
            last_run_date_local: Some(now_local.format("%Y-%m-%d").to_string()),
            last_run_at_utc: Some(Utc::now()),
            last_result: Some(last_result),
            last_error: result.as_ref().err().map(|e| e.to_string()),
            next_run_at_utc: None, // Will be computed next tick
        };
        
        // TODO: Save state to database
    }
}
