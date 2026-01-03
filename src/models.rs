//! Data models for Auto Open

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Target type - what to open
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Exe,
    File,
    Folder,
    Shortcut,
    Url,
}

/// Window style when running exe
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunWindowStyle {
    #[default]
    Normal,
    Minimized,
    Hidden,
}

/// Wait policy for exe execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WaitPolicy {
    DontWait,
    WaitForExit { timeout_seconds: Option<u32> },
}

impl Default for WaitPolicy {
    fn default() -> Self {
        WaitPolicy::DontWait
    }
}

/// Trigger types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    OnLogin {
        enabled: bool,
        delay_seconds: u32,
    },
    OncePerDay {
        enabled: bool,
        earliest_time_local: Option<String>, // "HH:MM"
        days_of_week: Option<Vec<String>>,   // ["Mon", "Tue", ...]
    },
    DailyAt {
        enabled: bool,
        time_local: String, // "HH:MM"
        days_of_week: Option<Vec<String>>,
    },
    Interval {
        enabled: bool,
        every_seconds: u32,
        jitter_seconds: Option<u32>,
    },
}

/// Condition types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    NetworkAvailable,
    OnAcPower,
    ProcessNotRunning { process_name: String },
    OnlyIfPathExists,
    IdleForSeconds { seconds: u32 },
}

/// Misfire policy
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MisfirePolicy {
    #[default]
    RunImmediately,
    SkipIfLateOverSeconds { seconds: u32 },
}

/// Action when target is already running
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IfRunningAction {
    /// Skip this execution if already running
    #[default]
    Skip,
    /// Close the existing instance and start a new one
    Restart,
    /// Run another instance anyway
    RunAnyway,
}

/// Main Task struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub enabled: bool,
    pub name: String,
    pub description: Option<String>,
    
    // Target
    pub target_type: TargetType,
    pub path_or_url: String,
    pub args: Option<String>,
    pub working_dir: Option<String>,
    
    // Execution options
    pub start_delay_seconds: u32,
    pub run_window_style: RunWindowStyle,
    pub wait_policy: WaitPolicy,
    pub singleton: bool,
    pub priority: Option<i32>,
    pub max_retries: u8,
    pub retry_backoff_seconds: u32,
    pub success_exit_codes: Option<Vec<i32>>,
    pub misfire_policy: MisfirePolicy,
    pub if_running_action: IfRunningAction,
    
    // Triggers and conditions
    pub triggers: Vec<Trigger>,
    pub conditions: Vec<Condition>,
    
    // Timestamps
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            enabled: true,
            name: String::new(),
            description: None,
            target_type: TargetType::File,
            path_or_url: String::new(),
            args: None,
            working_dir: None,
            start_delay_seconds: 0,
            run_window_style: RunWindowStyle::default(),
            wait_policy: WaitPolicy::default(),
            singleton: true,
            priority: None,
            max_retries: 0,
            retry_backoff_seconds: 10,
            success_exit_codes: Some(vec![0]),
            misfire_policy: MisfirePolicy::default(),
            if_running_action: IfRunningAction::default(),
            triggers: vec![],
            conditions: vec![],
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
        }
    }
}

/// Task state (runtime)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskState {
    pub task_id: String,
    pub last_run_date_local: Option<String>, // "YYYY-MM-DD"
    pub last_run_at_utc: Option<DateTime<Utc>>,
    pub last_result: Option<RunResult>,
    pub last_error: Option<String>,
    pub next_run_at_utc: Option<DateTime<Utc>>,
}

/// Run result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunResult {
    Success,
    Failed,
    Skipped,
}

/// Skip reason
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Disabled,
    ConditionFail,
    Singleton,
    MisfireSkip,
    PathMissing,
    AlreadyRanToday,
    DayNotAllowed,
    Paused,
    ManualOverride,
}

/// Run log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLog {
    pub run_id: String,
    pub task_id: String,
    pub task_name: String,
    pub trigger_type: String,
    pub scheduled_time_utc: Option<DateTime<Utc>>,
    pub started_at_utc: DateTime<Utc>,
    pub finished_at_utc: Option<DateTime<Utc>>,
    pub status: RunStatus,
    pub skip_reason: Option<SkipReason>,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub output: Option<String>,
}

/// Run status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Started,
    Success,
    Failed,
    Skipped,
}

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub start_with_windows: bool,
    pub start_minimized_to_tray: bool,
    pub show_notifications: bool,
    pub timezone_id: String,
    pub log_retention_days: u32,
    pub max_parallel_runs: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            start_minimized_to_tray: false,
            show_notifications: true,
            timezone_id: "system".to_string(),
            log_retention_days: 30,
            max_parallel_runs: 3,
        }
    }
}
