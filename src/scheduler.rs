//! Scheduler module - Compute next run times and manage task scheduling

use crate::models::*;
use chrono::{DateTime, Datelike, Local, NaiveTime, TimeZone, Utc, Weekday};

/// Compute the next run time for a trigger
pub fn compute_next_run(
    trigger: &Trigger,
    now_local: DateTime<Local>,
    state: &TaskState,
) -> Option<DateTime<Utc>> {
    match trigger {
        Trigger::OnLogin { enabled, delay_seconds: _ } => {
            if !enabled {
                return None;
            }
            // OnLogin only runs at app startup, not scheduled
            None
        }
        
        Trigger::OncePerDay { enabled, earliest_time_local, days_of_week } => {
            if !enabled {
                return None;
            }
            
            // Check if already ran today
            let today = now_local.format("%Y-%m-%d").to_string();
            if state.last_run_date_local.as_ref() == Some(&today) {
                return None; // Already ran today
            }
            
            // Check day of week restriction
            if let Some(days) = days_of_week {
                let today_weekday = weekday_to_string(now_local.weekday());
                if !days.iter().any(|d| d.eq_ignore_ascii_case(&today_weekday)) {
                    return None; // Not the right day
                }
            }
            
            // Check earliest time
            if let Some(time_str) = earliest_time_local {
                if let Ok(earliest) = NaiveTime::parse_from_str(time_str, "%H:%M") {
                    let current_time = now_local.time();
                    if current_time < earliest {
                        // Schedule for earliest time today
                        let target = now_local.date_naive().and_time(earliest);
                        return Some(Local.from_local_datetime(&target).unwrap().with_timezone(&Utc));
                    }
                }
            }
            
            // Ready to run now
            Some(now_local.with_timezone(&Utc))
        }
        
        Trigger::DailyAt { enabled, time_local, days_of_week } => {
            if !enabled {
                return None;
            }
            
            let target_time = match NaiveTime::parse_from_str(time_local, "%H:%M") {
                Ok(t) => t,
                Err(_) => return None,
            };
            
            // Find next occurrence
            for day_offset in 0..8 {
                let target_date = (now_local + chrono::Duration::days(day_offset)).date_naive();
                let target_datetime = target_date.and_time(target_time);
                let target_local = match Local.from_local_datetime(&target_datetime).latest() {
                    Some(t) => t,
                    None => continue, // DST gap, skip
                };
                
                // Skip if in the past
                if target_local <= now_local {
                    continue;
                }
                
                // Check day of week restriction
                if let Some(days) = days_of_week {
                    let weekday = weekday_to_string(target_local.weekday());
                    if !days.iter().any(|d| d.eq_ignore_ascii_case(&weekday)) {
                        continue;
                    }
                }
                
                return Some(target_local.with_timezone(&Utc));
            }
            
            None
        }
        
        Trigger::Interval { enabled, every_seconds, jitter_seconds } => {
            if !enabled || *every_seconds < 60 {
                return None;
            }
            
            let base = state.last_run_at_utc.unwrap_or(now_local.with_timezone(&Utc));
            let next = base + chrono::Duration::seconds(*every_seconds as i64);
            
            // Add jitter if specified
            let next = if let Some(jitter) = jitter_seconds {
                let jitter_offset = rand_jitter(*jitter);
                next + chrono::Duration::seconds(jitter_offset as i64)
            } else {
                next
            };
            
            // If next is in the past, schedule for now
            if next <= now_local.with_timezone(&Utc) {
                Some(now_local.with_timezone(&Utc))
            } else {
                Some(next)
            }
        }
    }
}

fn weekday_to_string(wd: Weekday) -> String {
    match wd {
        Weekday::Mon => "Mon".to_string(),
        Weekday::Tue => "Tue".to_string(),
        Weekday::Wed => "Wed".to_string(),
        Weekday::Thu => "Thu".to_string(),
        Weekday::Fri => "Fri".to_string(),
        Weekday::Sat => "Sat".to_string(),
        Weekday::Sun => "Sun".to_string(),
    }
}

fn rand_jitter(max: u32) -> u32 {
    // Simple pseudo-random based on current time
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() % max) as u32
}

/// Check if a task should be skipped due to misfire policy
pub fn check_misfire(
    policy: &MisfirePolicy,
    scheduled: DateTime<Utc>,
    now: DateTime<Utc>,
) -> bool {
    match policy {
        MisfirePolicy::RunImmediately => false, // Don't skip
        MisfirePolicy::SkipIfLateOverSeconds { seconds } => {
            let late_by = (now - scheduled).num_seconds();
            late_by > *seconds as i64
        }
    }
}
