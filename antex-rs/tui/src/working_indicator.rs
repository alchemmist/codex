use ratatui::style::Stylize;
use ratatui::text::Line;
use std::time::{Duration, Instant};

#[path = "working_shimmer.rs"]
mod shimmer;

pub(crate) struct WorkingIndicator {
    started: Option<Instant>,
    pub animations: bool,
}

impl Default for WorkingIndicator {
    fn default() -> Self {
        Self {
            started: None,
            animations: true,
        }
    }
}

impl WorkingIndicator {
    pub fn set_running(&mut self, running: bool) {
        if running {
            self.started.get_or_insert_with(Instant::now);
        } else {
            self.started = None;
        }
    }

    pub fn is_running(&self) -> bool {
        self.started.is_some()
    }

    pub fn line(&self, width: u16) -> Option<Line<'static>> {
        self.started
            .map(|start| self.render(start.elapsed(), width))
    }

    fn render(&self, elapsed: Duration, width: u16) -> Line<'static> {
        let mut spans = Vec::new();
        if self.animations {
            let indicator = if supports_color::on_cached(supports_color::Stream::Stdout)
                .is_some_and(|level| level.has_16m)
            {
                shimmer::shimmer_spans("•")
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| "•".into())
            } else if (elapsed.as_millis() / 600).is_multiple_of(2) {
                "•".into()
            } else {
                "◦".dim()
            };
            spans.extend([indicator, " ".into()]);
            spans.extend(shimmer::shimmer_spans("Working"));
        } else {
            spans.push("Working".into());
        }
        spans.extend([
            format!(" ({} • ", fmt_elapsed_compact(elapsed.as_secs())).dim(),
            "esc".into(),
            " to interrupt)".dim(),
        ]);
        crate::line_truncation::truncate_line_with_ellipsis_if_overflow(
            Line::from(spans),
            usize::from(width),
        )
    }
}

fn fmt_elapsed_compact(elapsed_secs: u64) -> String {
    if elapsed_secs < 60 {
        return format!("{elapsed_secs}s");
    }
    if elapsed_secs < 3600 {
        let minutes = elapsed_secs / 60;
        let seconds = elapsed_secs % 60;
        return format!("{minutes}m {seconds:02}s");
    }
    let hours = elapsed_secs / 3600;
    let minutes = (elapsed_secs % 3600) / 60;
    let seconds = elapsed_secs % 60;
    format!("{hours}h {minutes:02}m {seconds:02}s")
}

#[cfg(test)]
#[path = "working_indicator_tests.rs"]
mod tests;
