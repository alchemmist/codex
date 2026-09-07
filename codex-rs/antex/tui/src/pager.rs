use std::path::PathBuf;
use std::sync::Arc;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::key_hint::KeyBindingListExt;
use crate::keymap::RuntimeKeymap;
use crate::overlay::OverlayAction;
use crate::terminal_hyperlinks::HyperlinkLine;
use crate::transcript::safe_text;

pub struct TextPage {
    pub title: String,
    pub body: String,
    pub older_command: Option<String>,
}

pub(crate) struct Pager {
    page: TextPage,
    cwd: PathBuf,
    keymap: Arc<RuntimeKeymap>,
    offset: usize,
    height: usize,
    width: u16,
    lines: Vec<HyperlinkLine>,
    revision: u64,
}

impl Pager {
    pub(crate) fn new(
        page: TextPage,
        cwd: PathBuf,
        keymap: Arc<RuntimeKeymap>,
    ) -> Result<Self, String> {
        if page.title.len() > 128
            || page.body.len() > 64 * 1024
            || page
                .older_command
                .as_ref()
                .is_some_and(|command| command.len() > 4096 || !command.starts_with('/'))
        {
            return Err("Transcript page exceeds its limits.".into());
        }
        Ok(Self {
            page,
            cwd,
            keymap,
            offset: 0,
            height: 1,
            width: 0,
            lines: Vec::new(),
            revision: 0,
        })
    }

    pub(crate) fn key(&mut self, key: KeyEvent) -> OverlayAction {
        let map = &self.keymap.pager;
        if map.close.is_pressed(key) || map.close_transcript.is_pressed(key) {
            return OverlayAction::Close;
        }
        if map.scroll_up.is_pressed(key) {
            self.offset = self.offset.saturating_sub(1);
        } else if map.scroll_down.is_pressed(key) {
            self.offset += 1;
        } else if map.page_up.is_pressed(key) {
            if self.offset == 0
                && let Some(command) = &self.page.older_command
            {
                return OverlayAction::Command(command.clone());
            }
            self.offset = self.offset.saturating_sub(self.height);
        } else if map.page_down.is_pressed(key) {
            self.offset += self.height;
        } else if map.half_page_up.is_pressed(key) {
            self.offset = self.offset.saturating_sub(self.height.div_ceil(2));
        } else if map.half_page_down.is_pressed(key) {
            self.offset += self.height.div_ceil(2);
        } else if map.jump_top.is_pressed(key) {
            self.offset = 0;
        } else if map.jump_bottom.is_pressed(key) {
            self.offset = self.lines.len();
        }
        self.offset = self
            .offset
            .min(self.lines.len().saturating_sub(self.height));
        OverlayAction::Continue
    }

    pub(crate) fn render(&mut self, area: Rect, buffer: &mut Buffer) {
        if area.height < 2 || area.width == 0 {
            return;
        }
        if self.width != area.width
            || self.revision != crate::render::highlight::syntax_theme_revision()
        {
            self.lines = crate::markdown::render_markdown_agent_with_links_and_cwd(
                &safe_text(&self.page.body),
                Some(usize::from(area.width)),
                Some(&self.cwd),
            );
            self.width = area.width;
            self.revision = crate::render::highlight::syntax_theme_revision();
        }
        self.height = usize::from(area.height - 2);
        self.offset = self
            .offset
            .min(self.lines.len().saturating_sub(self.height));
        Line::from(safe_text(&self.page.title).bold()).render(Rect { height: 1, ..area }, buffer);
        crate::terminal_hyperlinks::HyperlinkParagraph::new(
            &self.lines[self.offset..],
            Style::default(),
        )
        .render(
            Rect {
                y: area.y + 1,
                height: area.height - 2,
                ..area
            },
            buffer,
        );
        let help = if self.page.older_command.is_some() {
            "↑↓ scroll · PgUp at top loads older · Esc closes"
        } else {
            "↑↓ scroll · PgUp/PgDn · Esc closes"
        };
        Line::from(help.dim()).render(
            Rect {
                y: area.bottom() - 1,
                height: 1,
                ..area
            },
            buffer,
        );
    }
}

#[cfg(test)]
#[path = "pager_tests.rs"]
mod tests;
