//! Executor module - Execute tasks (open files, run apps, etc.)

use crate::models::*;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Path không tồn tại: {0}")]
    PathNotFound(String),
    
    #[error("Không thể mở: {0}")]
    OpenFailed(String),
    
    #[error("Process timeout sau {0} giây")]
    Timeout(u32),
    
    #[error("Exit code {0} không nằm trong danh sách success")]
    ExitCodeFailed(i32),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct ExecutionResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub output: Option<String>,
}

/// Execute a task
pub fn execute_task(task: &Task) -> Result<ExecutionResult, ExecutorError> {
    tracing::info!("Executing task: {} (type: {:?}, path: {})", task.name, task.target_type, task.path_or_url);
    
    // Check if path exists (for file-based targets)
    if matches!(task.target_type, TargetType::Exe | TargetType::File | TargetType::Folder | TargetType::Shortcut) {
        if !std::path::Path::new(&task.path_or_url).exists() {
            return Err(ExecutorError::PathNotFound(task.path_or_url.clone()));
        }
    }

    // Handle if_running_action for EXE type
    if matches!(task.target_type, TargetType::Exe) {
        let process_name = get_process_name(&task.path_or_url);
        let is_running = is_process_running(&process_name);
        
        if is_running {
            match task.if_running_action {
                IfRunningAction::Skip => {
                    tracing::info!("Task {} skipped - {} already running", task.name, process_name);
                    return Ok(ExecutionResult {
                        success: true,
                        exit_code: None,
                        error_message: Some(format!("Skipped - {} already running", process_name)),
                        output: None,
                    });
                }
                IfRunningAction::Restart => {
                    tracing::info!("Task {} - killing existing {} before restart", task.name, process_name);
                    kill_process(&process_name);
                    // Wait a bit for process to fully close
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                IfRunningAction::RunAnyway => {
                    tracing::info!("Task {} - running another instance of {}", task.name, process_name);
                    // Continue to execute
                }
            }
        }
    }

    match task.target_type {
        TargetType::Exe => execute_exe(task),
        TargetType::File | TargetType::Folder | TargetType::Shortcut | TargetType::Url => {
            execute_shell_open(task)
        }
    }
}

/// Get process name from path (e.g., "C:\\Program Files\\app.exe" -> "app.exe")
fn get_process_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}

/// Public version for use from commands
pub fn get_process_name_from_path(path: &str) -> String {
    get_process_name(path)
}

/// Check if a process is running by name
fn is_process_running(process_name: &str) -> bool {
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {}", process_name)])
            .output();
        
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.to_lowercase().contains(&process_name.to_lowercase())
            }
            Err(_) => false,
        }
    }
    
    #[cfg(not(windows))]
    {
        false
    }
}

/// Public version for use from commands
pub fn check_process_running(process_name: &str) -> bool {
    is_process_running(process_name)
}

/// Kill a process by name
fn kill_process(process_name: &str) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", process_name])
            .output();
    }
}

/// Execute an exe with arguments
fn execute_exe(task: &Task) -> Result<ExecutionResult, ExecutorError> {
    let mut cmd = Command::new(&task.path_or_url);
    
    // Add arguments
    if let Some(args) = &task.args {
        // Parse arguments properly (handle quoted strings)
        let parsed_args = parse_args(args);
        cmd.args(&parsed_args);
    }
    
    // Set working directory
    if let Some(wd) = &task.working_dir {
        cmd.current_dir(wd);
    } else {
        // Default to parent directory of the executable
        if let Some(parent) = std::path::Path::new(&task.path_or_url).parent() {
            cmd.current_dir(parent);
        }
    }
    
    // Set window style
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        match task.run_window_style {
            RunWindowStyle::Hidden => {
                cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }
            RunWindowStyle::Minimized => {
                // Minimized needs different handling via STARTUPINFO, or via "start /min" wrapper
                // For now, let's just leave it (it might run normal if we don't do complex winapi)
            }
            RunWindowStyle::Normal => {}
        }
    }

    match &task.wait_policy {
        WaitPolicy::DontWait => {
            // Spawn and don't wait
            let _child = cmd.spawn()?;
            Ok(ExecutionResult {
                success: true,
                exit_code: None,
                error_message: None,
                output: None,
            })
        }
        WaitPolicy::WaitForExit { timeout_seconds } => {
            if let Some(timeout) = timeout_seconds {
                // Wait with timeout
                let mut child = cmd.spawn()?;
                let start = std::time::Instant::now();
                let timeout_duration = std::time::Duration::from_secs(*timeout as u64);
                
                // Check process status with timeout
                loop {
                    // Check if timeout exceeded first
                    if start.elapsed() >= timeout_duration {
                        tracing::warn!("Process timeout after {} seconds, killing process", timeout);
                        let _ = child.kill();
                        let _ = child.wait(); // Clean up zombie process
                        return Err(ExecutorError::Timeout(*timeout));
                    }
                    
                    // Try to get process status
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let code = status.code().unwrap_or(-1);
                            let success = check_exit_code(code, &task.success_exit_codes);
                            tracing::info!("Process exited with code: {}", code);
                            return Ok(ExecutionResult {
                                success,
                                exit_code: Some(code),
                                error_message: if success { None } else { Some(format!("Exit code: {}", code)) },
                                output: None, 
                            });
                        }
                        Ok(None) => {
                            // Process still running, sleep and check again
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                        Err(e) => {
                            tracing::error!("Error checking process status: {}", e);
                            return Err(ExecutorError::IoError(e));
                        }
                    }
                }
            } else {
                // Wait indefinitely - Use output() to capture stdout/stderr
                // Note: output() handles waiting
                // However, we need to be careful if we wanted to hide window but output() might show console? 
                // No, output() just captures.
                
                // Important: On Windows, for GUI apps, output might be empty. For CLI, it works.
                // We MUST set creation_flags again if needed? command structure keeps it.
                
                let output = cmd.output()?;
                let code = output.status.code().unwrap_or(-1);
                let success = check_exit_code(code, &task.success_exit_codes);
                
                // Combine stdout and stderr
                let mut out_str = String::from_utf8_lossy(&output.stdout).to_string();
                let err_str = String::from_utf8_lossy(&output.stderr).to_string();
                if !err_str.is_empty() {
                    out_str.push_str("\n--- STDERR ---\n");
                    out_str.push_str(&err_str);
                }
                
                Ok(ExecutionResult {
                    success,
                    exit_code: Some(code),
                    error_message: if success { None } else { Some(format!("Exit code: {}", code)) },
                    output: Some(out_str),
                })
            }
        }
    }
}

/// Open file/folder/shortcut/url using shell
fn execute_shell_open(task: &Task) -> Result<ExecutionResult, ExecutorError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &task.path_or_url]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        
        let status = cmd.status()?;
        
        Ok(ExecutionResult {
            success: status.success(),
            exit_code: status.code(),
            error_message: if status.success() { None } else { Some("Failed to open".to_string()) },
            output: None,
        })
    }
    
    #[cfg(not(windows))]
    {
        Err(ExecutorError::OpenFailed("Only Windows is supported".to_string()))
    }
}

/// Parse command line arguments (handle quoted strings)
fn parse_args(args: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = args.chars().peekable();
    
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
            }
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }
    
    if !current.is_empty() {
        result.push(current);
    }
    
    result
}

/// Check if exit code is in success list
fn check_exit_code(code: i32, success_codes: &Option<Vec<i32>>) -> bool {
    match success_codes {
        Some(codes) => codes.contains(&code),
        None => code == 0,
    }
}
