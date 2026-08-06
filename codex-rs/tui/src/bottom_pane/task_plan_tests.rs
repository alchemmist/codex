use codex_protocol::plan_tool::PlanItemArg;
use codex_protocol::plan_tool::StepStatus;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::*;

fn item(step: &str, status: StepStatus) -> PlanItemArg {
    PlanItemArg {
        step: step.to_string(),
        status,
    }
}

fn render(plan: &TaskPlan, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    plan.render(area, &mut buffer);
    (0..height)
        .map(|row| {
            (0..width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn long_plan() -> TaskPlan {
    let mut plan = TaskPlan::default();
    plan.begin();
    plan.set(vec![
        item("First complete", StepStatus::Completed),
        item("Second complete", StepStatus::Completed),
        item("Third complete", StepStatus::Completed),
        item("Current task", StepStatus::InProgress),
        item("First upcoming", StepStatus::Pending),
        item("Second upcoming", StepStatus::Pending),
        item("Last upcoming", StepStatus::Pending),
    ]);
    plan
}

#[test]
fn inactive_plan_ignores_updates() {
    let mut plan = TaskPlan::default();

    plan.set(vec![item("Unexpected task", StepStatus::InProgress)]);

    assert_eq!(plan.visible_capacity(), 0);
    assert_eq!(render(&plan, 40, 1), "");
}

#[test]
fn five_row_window_centers_on_current_task() {
    let plan = long_plan();

    assert_eq!(plan.visible_range(5), 1..6);
    insta::assert_snapshot!(render(&plan, 40, 5), @r"
      ✓ Second complete
      ✓ Third complete
      ● Current task
      ○ First upcoming
      ○ Second upcoming
    ");
}

#[test]
fn compact_terminal_shows_only_current_task() {
    let plan = long_plan();
    plan.set_terminal_height(8);

    assert_eq!(plan.visible_range(plan.visible_capacity()), 3..4);
    insta::assert_snapshot!(render(&plan, 24, 1), @"  ● Current task");
}
