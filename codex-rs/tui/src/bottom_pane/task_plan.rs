use std::cell::Cell;
use std::ops::Range;

use codex_protocol::plan_tool::PlanItemArg;
use codex_protocol::plan_tool::StepStatus;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::render::renderable::Renderable;

const MAX_VISIBLE_TASKS: usize = 5;

#[derive(Debug)]
pub(super) struct TaskPlan {
    active: bool,
    plan: Vec<PlanItemArg>,
    terminal_height: Cell<u16>,
}

impl Default for TaskPlan {
    fn default() -> Self {
        Self {
            active: false,
            plan: Vec::new(),
            terminal_height: Cell::new(u16::MAX),
        }
    }
}

impl TaskPlan {
    pub(super) fn begin(&mut self) {
        self.active = true;
        self.plan = vec![PlanItemArg {
            step: "Preparing task plan".to_string(),
            status: StepStatus::InProgress,
        }];
    }

    pub(super) fn set(&mut self, plan: Vec<PlanItemArg>) {
        if !self.active {
            return;
        }
        self.plan = plan;
    }

    pub(super) fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn set_terminal_height(&self, height: u16) {
        self.terminal_height.set(height);
    }

    fn visible_capacity(&self) -> usize {
        let capacity = match self.terminal_height.get() {
            0..=10 => 1,
            11..=13 => 2,
            14..=17 => 3,
            _ => MAX_VISIBLE_TASKS,
        };
        capacity.min(self.plan.len())
    }

    fn visible_range(&self, capacity: usize) -> Range<usize> {
        if capacity >= self.plan.len() {
            return 0..self.plan.len();
        }

        let focus = self
            .plan
            .iter()
            .position(|item| matches!(item.status, StepStatus::InProgress))
            .or_else(|| {
                self.plan
                    .iter()
                    .position(|item| matches!(item.status, StepStatus::Pending))
            })
            .unwrap_or_else(|| self.plan.len().saturating_sub(1));
        let start = focus
            .saturating_sub(capacity / 2)
            .min(self.plan.len().saturating_sub(capacity));
        start..start + capacity
    }

    fn line(item: &PlanItemArg, width: u16) -> Line<'static> {
        let line: Line<'static> = match &item.status {
            StepStatus::Completed => vec!["  ✓ ".green(), item.step.clone().dim()].into(),
            StepStatus::InProgress => vec!["  ● ".cyan().bold(), item.step.clone().bold()].into(),
            StepStatus::Pending => vec!["  ○ ".dim(), item.step.clone().dim()].into(),
        };
        truncate_line_with_ellipsis_if_overflow(line, usize::from(width))
    }
}

impl Renderable for TaskPlan {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let capacity = usize::from(area.height).min(self.visible_capacity());
        if capacity == 0 {
            return;
        }
        let lines = self.plan[self.visible_range(capacity)]
            .iter()
            .map(|item| Self::line(item, area.width))
            .collect::<Vec<_>>();
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        u16::try_from(self.visible_capacity()).unwrap_or(u16::MAX)
    }
}

#[cfg(test)]
#[path = "task_plan_tests.rs"]
mod tests;
