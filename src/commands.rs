//! Tauri commands - Frontend <-> Backend bridge

use crate::models::*;
use crate::storage::Database;
use std::sync::OnceLock;
// use tauri::State;

static DB: OnceLock<Database> = OnceLock::new();

/// Initialize the database
pub fn init_database(data_dir: &std::path::Path) -> Result<(), String> {
    let db_path = data_dir.join("auto-open.db");
    let db = Database::open(&db_path).map_err(|e| e.to_string())?;
    DB.set(db).map_err(|_| "Database already initialized".to_string())?;
    Ok(())
}

fn get_db() -> Result<&'static Database, String> {
    DB.get().ok_or_else(|| "Database not initialized".to_string())
}

#[tauri::command]
pub async fn get_tasks() -> Result<Vec<Task>, String> {
    let db = get_db()?;
    db.get_all_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_task(task: Task) -> Result<Task, String> {
    let db = get_db()?;
    let mut new_task = task;
    new_task.id = uuid::Uuid::new_v4().to_string();
    new_task.created_at_utc = chrono::Utc::now();
    new_task.updated_at_utc = chrono::Utc::now();
    
    db.insert_task(&new_task).map_err(|e| e.to_string())?;
    Ok(new_task)
}

#[tauri::command]
pub async fn update_task(task: Task) -> Result<(), String> {
    let db = get_db()?;
    db.update_task(&task).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_task(id: String) -> Result<(), String> {
    let db = get_db()?;
    db.delete_task(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_task_now(id: String) -> Result<(), String> {
    let db = get_db()?;
    let tasks = db.get_all_tasks().map_err(|e| e.to_string())?;
    
    let task = tasks.into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| "Task not found".to_string())?;
    
    tracing::info!("Running task now: {}", task.name);
    
    // Execute the task
    let result = crate::executor::execute_task(&task);
    
    // Log the execution
    let (status, error_message, exit_code, output) = match &result {
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
        trigger_type: "Manual".to_string(),
        scheduled_time_utc: None,
        started_at_utc: chrono::Utc::now(),
        finished_at_utc: Some(chrono::Utc::now()),
        status,
        skip_reason: None,
        exit_code,
        error_message: error_message.clone(),
        output,
    };
    
    let _ = db.insert_log(&log);
    
    match result {
        Ok(r) if r.success => Ok(()),
        Ok(r) => Err(r.error_message.unwrap_or_else(|| "Task failed".to_string())),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_logs() -> Result<Vec<RunLog>, String> {
    let db = get_db()?;
    db.get_logs(100).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_settings() -> Result<Settings, String> {
    let db = get_db()?;
    let mut settings = db.get_settings().map_err(|e| e.to_string())?;
    
    // Check actual autostart status from registry
    settings.start_with_windows = crate::autostart::is_autostart_enabled();
    
    Ok(settings)
}

#[tauri::command]
pub async fn update_settings(settings: Settings) -> Result<(), String> {
    let db = get_db()?;
    
    // Handle autostart separately
    crate::autostart::set_autostart(settings.start_with_windows)?;
    
    db.save_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_autostart_status() -> Result<bool, String> {
    Ok(crate::autostart::is_autostart_enabled())
}

#[tauri::command]
pub async fn set_autostart(enabled: bool) -> Result<(), String> {
    crate::autostart::set_autostart(enabled)
}

#[tauri::command]
pub async fn save_config_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}
