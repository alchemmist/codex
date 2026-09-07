use antex_core::ContextFragment;
use antex_core::ContextKind;
use antex_core::ErrorKind;
use antex_core::MAX_TEXT_BYTES;
use antex_core::ToolOutcome;
use antex_core::ToolOutput;
use pretty_assertions::assert_eq;

#[test]
fn context_rejects_oversized_utf8_input_instead_of_silently_changing_instructions() {
    let text = "я".repeat(MAX_TEXT_BYTES);
    let error = ContextFragment::new(ContextKind::Project, text).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Limit);
}

#[test]
fn short_tool_results_remain_exact() {
    let output = ToolOutput::new("call".into(), ToolOutcome::Failure, "не найдено".into());
    assert_eq!(
        (&*output.call_id, output.outcome, output.text()),
        ("call", ToolOutcome::Failure, "не найдено")
    );
}

#[test]
fn long_tool_output_preserves_a_valid_prefix_and_marks_truncation() {
    for text in ["x".repeat(MAX_TEXT_BYTES * 2), "🦀я".repeat(MAX_TEXT_BYTES)] {
        let output = ToolOutput::new("call".into(), ToolOutcome::Success, text.clone());
        let prefix = output.text().strip_suffix("\n[output truncated]").unwrap();
        assert!(text.starts_with(prefix));
        assert!(output.text().len() <= MAX_TEXT_BYTES);
        assert!(!prefix.is_empty());
    }
}
