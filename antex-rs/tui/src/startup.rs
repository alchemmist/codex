use ratatui::style::Stylize;
use ratatui::text::Line;

use crate::StartupMascotSkin;
use crate::mascot::MascotFrame;
use crate::mascot::StartupMascotMotion;
use crate::mascot::render_mascot;
use crate::tui::FrameRequester;

pub(crate) struct Startup {
    skin: StartupMascotSkin,
    title: String,
    motion: Option<StartupMascotMotion>,
}

impl Startup {
    pub(crate) fn new(settings: &crate::Settings, frames: FrameRequester) -> Self {
        Self {
            skin: settings.mascot,
            title: settings.title.clone(),
            motion: settings
                .animations
                .then(|| StartupMascotMotion::new(frames)),
        }
    }

    pub(crate) fn lines(&self, width: u16, view: &crate::SessionView) -> Vec<Line<'static>> {
        self.render(
            width,
            self.motion
                .as_ref()
                .map(StartupMascotMotion::current_frame)
                .unwrap_or(MascotFrame::Rest),
            view,
        )
    }

    pub(crate) fn final_lines(&self, width: u16, view: &crate::SessionView) -> Vec<Line<'static>> {
        self.render(width, MascotFrame::Rest, view)
    }

    fn render(
        &self,
        width: u16,
        frame: MascotFrame,
        view: &crate::SessionView,
    ) -> Vec<Line<'static>> {
        use crate::line_truncation::{line_width, truncate_line_with_ellipsis_if_overflow};
        let title = format!("{} · {}", self.title, env!("CARGO_PKG_VERSION"));
        if width < 8 {
            return vec![truncate_line_with_ellipsis_if_overflow(
                Line::from(title).bold(),
                usize::from(width),
            )];
        }
        let mut content = vec![
            Line::from(view.model.clone()),
            Line::from(crate::transcript::display_directory(&view.directory)),
        ];
        if view.permissions.eq_ignore_ascii_case("full") {
            content.push(Line::from("YOLO"));
        }
        let content_width = content
            .iter()
            .map(line_width)
            .chain(std::iter::once(crate::width::display_width(&title) + 4))
            .max()
            .unwrap_or(0)
            .min(usize::from(width.saturating_sub(6)).min(54));
        let title = truncate_line_with_ellipsis_if_overflow(
            Line::from(title),
            content_width.saturating_sub(4).max(1),
        )
        .to_string();
        let border_width = content_width + 4;
        let remaining = border_width.saturating_sub(crate::width::display_width(&title) + 3);
        let mut panel = vec![Line::from(vec![
            "╭─ ".dim(),
            title.bold(),
            " ".dim(),
            format!("{}╮", "─".repeat(remaining)).dim(),
        ])];
        for line in content {
            let line = truncate_line_with_ellipsis_if_overflow(line, content_width);
            let used_width = line_width(&line);
            let mut spans = vec!["│  ".dim()];
            spans.extend(line.spans);
            spans.push(" ".repeat(content_width.saturating_sub(used_width)).into());
            spans.push("  │".dim());
            panel.push(Line::from(spans));
        }
        panel.push(Line::from(format!("╰{}╯", "─".repeat(border_width)).dim()));
        let mascot = render_mascot(self.skin, frame);
        let panel_width = panel.iter().map(line_width).max().unwrap_or(0);
        if mascot.is_empty() || crate::mascot::MASCOT_WIDTH + 2 + panel_width > usize::from(width) {
            panel.push(Line::default());
            return panel;
        }
        let height = mascot.len().max(panel.len());
        let mascot_top = height.saturating_sub(mascot.len()) / 2;
        let panel_top = height.saturating_sub(panel.len()) / 2;
        let mut lines = (0..height)
            .map(|row| {
                let mascot_line = row.checked_sub(mascot_top).and_then(|row| mascot.get(row));
                let panel_line = row.checked_sub(panel_top).and_then(|row| panel.get(row));
                let mut spans = mascot_line
                    .map(|line| line.spans.clone())
                    .unwrap_or_default();
                if let Some(line) = panel_line {
                    let used = mascot_line.map(line_width).unwrap_or(0);
                    spans.push(
                        " ".repeat(crate::mascot::MASCOT_WIDTH.saturating_sub(used) + 2)
                            .into(),
                    );
                    spans.extend(line.spans.clone());
                }
                Line::from(spans)
            })
            .collect::<Vec<_>>();
        lines.push(Line::default());
        lines
    }
}
