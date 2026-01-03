//! Storage module - SQLite database operations

use crate::models::*;
use rusqlite::{Connection, params, Result};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open or create database at path
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn: Mutex::new(conn) };
        db.run_migrations()?;
        Ok(db)
    }

    /// Run database migrations
    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 1,
                name TEXT NOT NULL,
                description TEXT,
                target_type TEXT NOT NULL,
                path_or_url TEXT NOT NULL,
                args TEXT,
                working_dir TEXT,
                start_delay_seconds INTEGER DEFAULT 0,
                run_window_style TEXT DEFAULT 'normal',
                wait_policy TEXT DEFAULT '{"type":"dont_wait"}',
                singleton INTEGER DEFAULT 1,
                priority INTEGER,
                max_retries INTEGER DEFAULT 0,
                retry_backoff_seconds INTEGER DEFAULT 10,
                success_exit_codes TEXT,
                misfire_policy TEXT DEFAULT '{"type":"run_immediately"}',
                if_running_action TEXT DEFAULT 'skip',
                triggers TEXT NOT NULL DEFAULT '[]',
                conditions TEXT NOT NULL DEFAULT '[]',
                created_at_utc TEXT NOT NULL,
                updated_at_utc TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS task_state (
                task_id TEXT PRIMARY KEY,
                last_run_date_local TEXT,
                last_run_at_utc TEXT,
                last_result TEXT,
                last_error TEXT,
                next_run_at_utc TEXT,
                FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS run_logs (
                run_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                task_name TEXT NOT NULL,
                trigger_type TEXT,
                scheduled_time_utc TEXT,
                started_at_utc TEXT NOT NULL,
                finished_at_utc TEXT,
                status TEXT NOT NULL,
                skip_reason TEXT,
                exit_code INTEGER,
                error_message TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_run_logs_task_id ON run_logs(task_id);
            CREATE INDEX IF NOT EXISTS idx_run_logs_started_at ON run_logs(started_at_utc);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
        "#)?;
        Ok(())
    }

    // === Task CRUD ===

    pub fn get_all_tasks(&self) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, enabled, name, description, target_type, path_or_url, args, working_dir,
                    start_delay_seconds, run_window_style, wait_policy, singleton, priority,
                    max_retries, retry_backoff_seconds, success_exit_codes, misfire_policy,
                    if_running_action, triggers, conditions, created_at_utc, updated_at_utc
             FROM tasks ORDER BY name"
        )?;
        
        let tasks = stmt.query_map([], |row| {
            Ok(Task {
                id: row.get(0)?,
                enabled: row.get::<_, i32>(1)? != 0,
                name: row.get(2)?,
                description: row.get(3)?,
                target_type: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(TargetType::File),
                path_or_url: row.get(5)?,
                args: row.get(6)?,
                working_dir: row.get(7)?,
                start_delay_seconds: row.get::<_, i32>(8)? as u32,
                run_window_style: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
                wait_policy: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
                singleton: row.get::<_, i32>(11)? != 0,
                priority: row.get(12)?,
                max_retries: row.get::<_, i32>(13)? as u8,
                retry_backoff_seconds: row.get::<_, i32>(14)? as u32,
                success_exit_codes: row.get::<_, Option<String>>(15)?
                    .and_then(|s| serde_json::from_str(&s).ok()),
                misfire_policy: serde_json::from_str(&row.get::<_, String>(16)?).unwrap_or_default(),
                if_running_action: serde_json::from_str(&row.get::<_, String>(17)?).unwrap_or_default(),
                triggers: serde_json::from_str(&row.get::<_, String>(18)?).unwrap_or_default(),
                conditions: serde_json::from_str(&row.get::<_, String>(19)?).unwrap_or_default(),
                created_at_utc: row.get::<_, String>(20)?.parse().unwrap_or_else(|_| chrono::Utc::now()),
                updated_at_utc: row.get::<_, String>(21)?.parse().unwrap_or_else(|_| chrono::Utc::now()),
            })
        })?.collect::<Result<Vec<_>>>()?;
        
        Ok(tasks)
    }

    pub fn insert_task(&self, task: &Task) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tasks (id, enabled, name, description, target_type, path_or_url, args, working_dir,
                start_delay_seconds, run_window_style, wait_policy, singleton, priority,
                max_retries, retry_backoff_seconds, success_exit_codes, misfire_policy,
                if_running_action, triggers, conditions, created_at_utc, updated_at_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                task.id,
                task.enabled as i32,
                task.name,
                task.description,
                serde_json::to_string(&task.target_type).unwrap(),
                task.path_or_url,
                task.args,
                task.working_dir,
                task.start_delay_seconds as i32,
                serde_json::to_string(&task.run_window_style).unwrap(),
                serde_json::to_string(&task.wait_policy).unwrap(),
                task.singleton as i32,
                task.priority,
                task.max_retries as i32,
                task.retry_backoff_seconds as i32,
                task.success_exit_codes.as_ref().map(|v| serde_json::to_string(v).unwrap()),
                serde_json::to_string(&task.misfire_policy).unwrap(),
                serde_json::to_string(&task.if_running_action).unwrap(),
                serde_json::to_string(&task.triggers).unwrap(),
                serde_json::to_string(&task.conditions).unwrap(),
                task.created_at_utc.to_rfc3339(),
                task.updated_at_utc.to_rfc3339(),
            ]
        )?;
        Ok(())
    }

    pub fn update_task(&self, task: &Task) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET enabled=?2, name=?3, description=?4, target_type=?5, path_or_url=?6,
                args=?7, working_dir=?8, start_delay_seconds=?9, run_window_style=?10, wait_policy=?11,
                singleton=?12, priority=?13, max_retries=?14, retry_backoff_seconds=?15, success_exit_codes=?16,
                misfire_policy=?17, if_running_action=?18, triggers=?19, conditions=?20, updated_at_utc=?21
             WHERE id=?1",
            params![
                task.id,
                task.enabled as i32,
                task.name,
                task.description,
                serde_json::to_string(&task.target_type).unwrap(),
                task.path_or_url,
                task.args,
                task.working_dir,
                task.start_delay_seconds as i32,
                serde_json::to_string(&task.run_window_style).unwrap(),
                serde_json::to_string(&task.wait_policy).unwrap(),
                task.singleton as i32,
                task.priority,
                task.max_retries as i32,
                task.retry_backoff_seconds as i32,
                task.success_exit_codes.as_ref().map(|v| serde_json::to_string(v).unwrap()),
                serde_json::to_string(&task.misfire_policy).unwrap(),
                serde_json::to_string(&task.if_running_action).unwrap(),
                serde_json::to_string(&task.triggers).unwrap(),
                serde_json::to_string(&task.conditions).unwrap(),
                chrono::Utc::now().to_rfc3339(),
            ]
        )?;
        Ok(())
    }

    pub fn delete_task(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        conn.execute("DELETE FROM task_state WHERE task_id = ?1", params![id])?;
        Ok(())
    }

    // === Run Logs ===

    pub fn get_logs(&self, limit: u32) -> Result<Vec<RunLog>> {
        let conn = self.conn.lock().unwrap();
        // Check if output column exists, if not add it (simple migration)
        let _ = conn.execute("ALTER TABLE run_logs ADD COLUMN output TEXT", []);

        let mut stmt = conn.prepare(
            "SELECT run_id, task_id, task_name, trigger_type, scheduled_time_utc,
                    started_at_utc, finished_at_utc, status, skip_reason, exit_code, error_message, output
             FROM run_logs ORDER BY started_at_utc DESC LIMIT ?1"
        )?;
        
        let logs = stmt.query_map([limit], |row| {
            Ok(RunLog {
                run_id: row.get(0)?,
                task_id: row.get(1)?,
                task_name: row.get(2)?,
                trigger_type: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                scheduled_time_utc: row.get::<_, Option<String>>(4)?
                    .and_then(|s| s.parse().ok()),
                started_at_utc: row.get::<_, String>(5)?.parse().unwrap_or_else(|_| chrono::Utc::now()),
                finished_at_utc: row.get::<_, Option<String>>(6)?
                    .and_then(|s| s.parse().ok()),
                status: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or(RunStatus::Failed),
                skip_reason: row.get::<_, Option<String>>(8)?
                    .and_then(|s| serde_json::from_str(&s).ok()),
                exit_code: row.get(9)?,
                error_message: row.get(10)?,
                output: row.get(11)?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        
        Ok(logs)
    }

    pub fn insert_log(&self, log: &RunLog) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO run_logs (run_id, task_id, task_name, trigger_type, scheduled_time_utc,
                started_at_utc, finished_at_utc, status, skip_reason, exit_code, error_message, output)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                log.run_id,
                log.task_id,
                log.task_name,
                log.trigger_type,
                log.scheduled_time_utc.map(|t| t.to_rfc3339()),
                log.started_at_utc.to_rfc3339(),
                log.finished_at_utc.map(|t| t.to_rfc3339()),
                serde_json::to_string(&log.status).unwrap(),
                log.skip_reason.as_ref().map(|r| serde_json::to_string(r).unwrap()),
                log.exit_code,
                log.error_message,
                log.output,
            ]
        )?;
        Ok(())
    }

    // === Settings ===

    pub fn get_settings(&self) -> Result<Settings> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut settings = Settings::default();
        for row in rows {
            let (key, value) = row?;
            match key.as_str() {
                "start_with_windows" => settings.start_with_windows = value == "true",
                "start_minimized_to_tray" => settings.start_minimized_to_tray = value == "true",
                "show_notifications" => settings.show_notifications = value == "true",
                "timezone_id" => settings.timezone_id = value,
                "log_retention_days" => settings.log_retention_days = value.parse().unwrap_or(30),
                "max_parallel_runs" => settings.max_parallel_runs = value.parse().unwrap_or(3),
                _ => {}
            }
        }
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let pairs = [
            ("start_with_windows", settings.start_with_windows.to_string()),
            ("start_minimized_to_tray", settings.start_minimized_to_tray.to_string()),
            ("show_notifications", settings.show_notifications.to_string()),
            ("timezone_id", settings.timezone_id.clone()),
            ("log_retention_days", settings.log_retention_days.to_string()),
            ("max_parallel_runs", settings.max_parallel_runs.to_string()),
        ];

        for (key, value) in pairs {
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value]
            )?;
        }
        Ok(())
    }
}
