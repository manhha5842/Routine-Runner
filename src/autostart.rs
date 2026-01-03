//! Autostart module - Manage Windows startup

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

const REGISTRY_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "AutoOpen";

/// Enable autostart with Windows
pub fn enable_autostart() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get exe path: {}", e))?;
        
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(REGISTRY_KEY, KEY_WRITE)
            .map_err(|e| format!("Failed to open registry key: {}", e))?;
        
        // Add --tray flag to start minimized
        let value = format!("\"{}\" --tray", exe_path.display());
        key.set_value(APP_NAME, &value)
            .map_err(|e| format!("Failed to set registry value: {}", e))?;
        
        tracing::info!("Autostart enabled");
        Ok(())
    }
    
    #[cfg(not(windows))]
    {
        Err("Autostart is only supported on Windows".to_string())
    }
}

/// Disable autostart
pub fn disable_autostart() -> Result<(), String> {
    #[cfg(windows)]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(REGISTRY_KEY, KEY_WRITE)
            .map_err(|e| format!("Failed to open registry key: {}", e))?;
        
        // Ignore error if value doesn't exist
        let _ = key.delete_value(APP_NAME);
        
        tracing::info!("Autostart disabled");
        Ok(())
    }
    
    #[cfg(not(windows))]
    {
        Err("Autostart is only supported on Windows".to_string())
    }
}

/// Check if autostart is currently enabled
pub fn is_autostart_enabled() -> bool {
    #[cfg(windows)]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey_with_flags(REGISTRY_KEY, KEY_READ) {
            let value: Result<String, _> = key.get_value(APP_NAME);
            return value.is_ok();
        }
        false
    }
    
    #[cfg(not(windows))]
    {
        false
    }
}

/// Set autostart based on boolean
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        enable_autostart()
    } else {
        disable_autostart()
    }
}
