use ratatui::style::Stylize;
use ratatui::text::Line;

use crate::StartupMascotSkin;
use crate::mascot::MascotFrame;
use crate::mascot::StartupMascotMotion;
use crate::mascot::render_mascot;
use crate::tui::FrameRequester;

pub(crate) struct Startup {
    skin: StartupMascotSkin,
    motion: StartupMascotMotion,
}

impl Startup {
    pub(crate) fn new(skin: StartupMascotSkin, frames: FrameRequester) -> Self {
        Self {
            skin,
            motion: StartupMascotMotion::new(frames),
        }
    }

    pub(crate) fn lines(&self, width: u16) -> Vec<Line<'static>> {
        self.render(width, self.motion.current_frame())
    }

    pub(crate) fn final_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.render(width, MascotFrame::Rest)
    }

    fn render(&self, width: u16, frame: MascotFrame) -> Vec<Line<'static>> {
        let title = vec![
            "Antex ".bold(),
            format!("v{}", env!("CARGO_PKG_VERSION")).dim(),
        ];
        if width < 36 || self.skin == StartupMascotSkin::None {
            return vec![Line::from(title), Line::default()];
        }
        render_mascot(self.skin, frame)
            .into_iter()
            .enumerate()
            .map(|(index, mascot)| {
                let mut spans = mascot.spans;
                spans.push("  ".into());
                match index {
                    0 => spans.extend(title.clone()),
                    2 => spans.push("Terminal coding agent".dim()),
                    4 => spans.push("/help for commands".dim()),
                    _ => {}
                }
                Line::from(spans)
            })
            .collect()
    }
}
