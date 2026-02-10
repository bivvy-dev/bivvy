//! Step progress display.

use std::time::{Duration, Instant};

use super::{BivvyTheme, ProgressSpinner, SpinnerHandle};

/// Displays progress for a step execution.
pub struct StepProgress {
    name: String,
    description: String,
    spinner: Option<Box<dyn SpinnerHandle>>,
    start_time: Instant,
    theme: BivvyTheme,
}

impl StepProgress {
    /// Create a new step progress display.
    pub fn new(name: &str, description: &str, show_spinner: bool) -> Self {
        let spinner = if show_spinner {
            Some(
                Box::new(ProgressSpinner::new(&format!("{} - {}", name, description)))
                    as Box<dyn SpinnerHandle>,
            )
        } else {
            None
        };

        Self {
            name: name.to_string(),
            description: description.to_string(),
            spinner,
            start_time: Instant::now(),
            theme: BivvyTheme::new(),
        }
    }

    /// Create without spinner (for quiet/silent modes).
    pub fn quiet(name: &str, description: &str) -> Self {
        Self::new(name, description, false)
    }

    /// Update the progress message.
    pub fn update(&mut self, msg: &str) {
        if let Some(spinner) = &mut self.spinner {
            spinner.set_message(msg);
        }
    }

    /// Mark as successful.
    pub fn success(mut self) {
        let duration = self.start_time.elapsed();
        let msg = format!(
            "{} - {} ({})",
            self.name,
            self.description,
            format_duration(duration)
        );

        if let Some(mut spinner) = self.spinner.take() {
            spinner.finish_success(&msg);
        } else {
            println!("{}", self.theme.format_success(&msg));
        }
    }

    /// Mark as failed.
    pub fn error(mut self, error: &str) {
        let msg = format!("{} - {}", self.name, error);

        if let Some(mut spinner) = self.spinner.take() {
            spinner.finish_error(&msg);
        } else {
            println!("{}", self.theme.format_error(&msg));
        }
    }

    /// Mark as skipped.
    pub fn skipped(mut self, reason: &str) {
        let msg = format!("{} - {}", self.name, reason);

        if let Some(mut spinner) = self.spinner.take() {
            spinner.finish_skipped(&msg);
        } else {
            println!("○ {}", msg);
        }
    }

    /// Get elapsed duration.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

/// Format a timestamp as a relative time string (e.g., "2 minutes ago").
pub fn format_relative_time(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(timestamp);
    let seconds = diff.num_seconds();

    if seconds < 0 {
        return "just now".to_string();
    }

    if seconds < 60 {
        return "just now".to_string();
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return if minutes == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", minutes)
        };
    }

    let hours = minutes / 60;
    if hours < 24 {
        return if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        };
    }

    let days = hours / 24;
    if days < 30 {
        return if days == 1 {
            "yesterday".to_string()
        } else {
            format!("{} days ago", days)
        };
    }

    let months = days / 30;
    if months < 12 {
        return if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{} months ago", months)
        };
    }

    let years = months / 12;
    if years == 1 {
        "1 year ago".to_string()
    } else {
        format!("{} years ago", years)
    }
}

/// Format a duration for display.
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 1.0 {
        format!("{}ms", d.as_millis())
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else {
        let mins = secs / 60.0;
        format!("{:.1}m", mins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_just_now() {
        let now = chrono::Utc::now();
        assert_eq!(format_relative_time(now), "just now");
    }

    #[test]
    fn relative_time_seconds_ago() {
        let ts = chrono::Utc::now() - chrono::Duration::seconds(30);
        assert_eq!(format_relative_time(ts), "just now");
    }

    #[test]
    fn relative_time_one_minute() {
        let ts = chrono::Utc::now() - chrono::Duration::minutes(1);
        assert_eq!(format_relative_time(ts), "1 minute ago");
    }

    #[test]
    fn relative_time_minutes() {
        let ts = chrono::Utc::now() - chrono::Duration::minutes(15);
        assert_eq!(format_relative_time(ts), "15 minutes ago");
    }

    #[test]
    fn relative_time_one_hour() {
        let ts = chrono::Utc::now() - chrono::Duration::hours(1);
        assert_eq!(format_relative_time(ts), "1 hour ago");
    }

    #[test]
    fn relative_time_hours() {
        let ts = chrono::Utc::now() - chrono::Duration::hours(5);
        assert_eq!(format_relative_time(ts), "5 hours ago");
    }

    #[test]
    fn relative_time_yesterday() {
        let ts = chrono::Utc::now() - chrono::Duration::days(1);
        assert_eq!(format_relative_time(ts), "yesterday");
    }

    #[test]
    fn relative_time_days() {
        let ts = chrono::Utc::now() - chrono::Duration::days(5);
        assert_eq!(format_relative_time(ts), "5 days ago");
    }

    #[test]
    fn relative_time_one_month() {
        let ts = chrono::Utc::now() - chrono::Duration::days(35);
        assert_eq!(format_relative_time(ts), "1 month ago");
    }

    #[test]
    fn relative_time_months() {
        let ts = chrono::Utc::now() - chrono::Duration::days(90);
        assert_eq!(format_relative_time(ts), "3 months ago");
    }

    #[test]
    fn relative_time_one_year() {
        let ts = chrono::Utc::now() - chrono::Duration::days(400);
        assert_eq!(format_relative_time(ts), "1 year ago");
    }

    #[test]
    fn relative_time_future_shows_just_now() {
        let ts = chrono::Utc::now() + chrono::Duration::hours(1);
        assert_eq!(format_relative_time(ts), "just now");
    }

    #[test]
    fn format_duration_milliseconds() {
        let d = Duration::from_millis(500);
        assert_eq!(format_duration(d), "500ms");
    }

    #[test]
    fn format_duration_seconds() {
        let d = Duration::from_secs_f64(5.3);
        assert_eq!(format_duration(d), "5.3s");
    }

    #[test]
    fn format_duration_minutes() {
        let d = Duration::from_secs(90);
        assert_eq!(format_duration(d), "1.5m");
    }

    #[test]
    fn format_duration_zero() {
        let d = Duration::ZERO;
        assert_eq!(format_duration(d), "0ms");
    }

    #[test]
    fn step_progress_without_spinner() {
        let progress = StepProgress::quiet("test", "Test step");
        progress.success();
    }

    #[test]
    fn step_progress_elapsed() {
        let progress = StepProgress::quiet("test", "Test step");
        std::thread::sleep(Duration::from_millis(10));
        assert!(progress.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn step_progress_error_without_spinner() {
        let progress = StepProgress::quiet("test", "Test step");
        progress.error("Something went wrong");
    }

    #[test]
    fn step_progress_skipped_without_spinner() {
        let progress = StepProgress::quiet("test", "Test step");
        progress.skipped("Already done");
    }

    #[test]
    fn step_progress_update_without_spinner() {
        let mut progress = StepProgress::quiet("test", "Test step");
        progress.update("Working...");
        progress.success();
    }
}
