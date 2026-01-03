//! Conditions module - Evaluate pre-run conditions

use crate::models::Condition;
use std::process::Command;

/// Evaluate all conditions for a task
pub fn evaluate_conditions(conditions: &[Condition]) -> Result<bool, String> {
    for condition in conditions {
        if !evaluate_single_condition(condition)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Evaluate a single condition
fn evaluate_single_condition(condition: &Condition) -> Result<bool, String> {
    match condition {
        Condition::NetworkAvailable => check_network_available(),
        Condition::OnAcPower => check_on_ac_power(),
        Condition::ProcessNotRunning { process_name } => check_process_not_running(process_name),
        Condition::OnlyIfPathExists => Ok(true), // Path check is done in executor
        Condition::IdleForSeconds { seconds: _ } => Ok(true), // TODO: Implement idle check
    }
}

/// Check if network is available
fn check_network_available() -> Result<bool, String> {
    #[cfg(windows)]
    {
        // Try to check network connectivity using Windows API
        // Simple approach: check if we can resolve DNS
        use std::net::ToSocketAddrs;
        match "www.google.com:80".to_socket_addrs() {
            Ok(mut addrs) => Ok(addrs.next().is_some()),
            Err(_) => Ok(false),
        }
    }
    
    #[cfg(not(windows))]
    {
        Ok(true)
    }
}

/// Check if on AC power (not on battery)
fn check_on_ac_power() -> Result<bool, String> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::GetSystemPowerStatus;
        use windows::Win32::System::Power::SYSTEM_POWER_STATUS;
        
        let mut status = SYSTEM_POWER_STATUS::default();
        let result = unsafe { GetSystemPowerStatus(&mut status) };
        
        if result.is_ok() {
            // ACLineStatus: 0 = Offline (battery), 1 = Online (AC)
            Ok(status.ACLineStatus == 1)
        } else {
            // If we can't determine, assume it's OK
            Ok(true)
        }
    }
    
    #[cfg(not(windows))]
    {
        Ok(true)
    }
}

/// Check if a process is NOT running
fn check_process_not_running(process_name: &str) -> Result<bool, String> {
    #[cfg(windows)]
    {
        // Use tasklist command to check
        let output = Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {}", process_name)])
            .output();
        
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // If process is found, tasklist will show it in output
                // If not found, it shows "INFO: No tasks are running..."
                let is_running = stdout.to_lowercase().contains(&process_name.to_lowercase());
                Ok(!is_running)
            }
            Err(_) => Ok(true), // Assume not running if we can't check
        }
    }
    
    #[cfg(not(windows))]
    {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_network_check() {
        let result = check_network_available();
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_process_not_running() {
        // Check for a process that definitely doesn't exist
        let result = check_process_not_running("nonexistent_process_12345.exe");
        assert!(result.unwrap());
    }
}
