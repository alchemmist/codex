use super::*;
use pretty_assertions::assert_eq;

#[test]
fn working_row_uses_the_fork_elapsed_and_interrupt_format() {
    let indicator = WorkingIndicator {
        started: None,
        animations: false,
    };
    assert_eq!(
        indicator.render(Duration::from_secs(61), 80).to_string(),
        "Working (1m 01s • esc to interrupt)"
    );
    insta::assert_snapshot!(indicator.render(Duration::from_secs(61), 80).to_string(), @"Working (1m 01s • esc to interrupt)");
    assert_eq!(fmt_elapsed_compact(3609), "1h 00m 09s");
    assert_eq!(fmt_elapsed_compact(0), "0s");
    assert!(indicator.line(80).is_none());
}

#[test]
fn running_clock_is_retained_until_the_run_finishes() {
    let mut indicator = WorkingIndicator::default();
    indicator.set_running(true);
    let start = indicator.started;
    indicator.set_running(true);
    assert_eq!(indicator.started, start);
    indicator.set_running(false);
    assert!(indicator.line(80).is_none());
}
