use super::decode;
use super::encode;
use antex_core::Content;
use antex_core::ErrorKind;
use antex_core::Message;
use antex_core::ModelEvent;
use antex_core::ModelRequest;
use antex_core::RawToolCall;
use antex_core::ToolOutcome;
use antex_core::ToolOutput;
use antex_core::Usage;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn request_preserves_encrypted_reasoning_and_tool_exchange_without_duplicate_text() {
    let reasoning =
        json!({"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"opaque"});
    let assistant = json!({"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"checking"}]});
    let request = ModelRequest {
        model: "test-model".into(),
        reasoning: Some("high".into()),
        tools: Vec::new(),
        messages: vec![
            Message::User("hello".into()),
            Message::Assistant {
                content: vec![
                    Content::Reasoning("private summary".into()),
                    Content::Text("checking".into()),
                    Content::Continuation {
                        provider: "openai".into(),
                        data: serde_json::to_vec(&reasoning).unwrap().into(),
                    },
                    Content::Continuation {
                        provider: "openai".into(),
                        data: serde_json::to_vec(&assistant).unwrap().into(),
                    },
                ],
                tool_calls: vec![RawToolCall {
                    id: "call-1".into(),
                    name: "read".into(),
                    arguments: r#"{"path":"a"}"#.into(),
                }],
            },
            Message::Tool(ToolOutput::new(
                "call-1".into(),
                ToolOutcome::Success,
                "contents".into(),
            )),
        ],
    };
    assert_eq!(
        encode(request).unwrap(),
        json!({
            "model":"test-model","instructions":"","stream":true,"store":false,
            "tools":[],"tool_choice":"auto","parallel_tool_calls":false,
            "include":["reasoning.encrypted_content"],"reasoning":{"effort":"high","summary":"auto"},
            "input":[
                {"role":"user","content":[{"type":"input_text","text":"hello"}]},
                reasoning, assistant,
                {"type":"function_call","call_id":"call-1","name":"read","arguments":r#"{"path":"a"}"#},
                {"type":"function_call_output","call_id":"call-1","output":"contents"},
            ],
        })
    );
}

#[test]
fn deltas_and_complete_tool_arguments_are_distinct_events() {
    assert_eq!(
        decode(json!({"type":"response.output_text.delta","delta":"hello"})).unwrap(),
        vec![ModelEvent::Text("hello".into())]
    );
    assert_eq!(
        decode(json!({"type":"response.function_call_arguments.delta","delta":"partial"})).unwrap(),
        Vec::new()
    );
    assert_eq!(decode(json!({"type":"response.output_item.done","item":{"type":"function_call","call_id":"c","name":"read","arguments":"{}"}})).unwrap(), vec![ModelEvent::ToolCall(RawToolCall { id:"c".into(),name:"read".into(),arguments:"{}".into() })]);
}

#[test]
fn completion_reports_usage_and_failure_never_becomes_completion() {
    assert_eq!(decode(json!({"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":12,"output_tokens":3,"input_tokens_details":{"cached_tokens":4}}}})).unwrap(), vec![ModelEvent::Finished(Usage {input_tokens:12,output_tokens:3,cached_input_tokens:4})]);
    for event in [
        json!({"type":"response.failed"}),
        json!({"type":"response.incomplete"}),
        json!({"type":"response.completed","response":{"status":"failed"}}),
    ] {
        assert_eq!(decode(event).unwrap_err().kind, ErrorKind::Protocol);
    }
}

#[test]
fn opaque_replay_cannot_bypass_text_budgets_or_forward_unknown_metadata() {
    let oversized = json!({"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"x".repeat(antex_core::MAX_TEXT_BYTES+1)}]}});
    assert_eq!(decode(oversized).unwrap_err().kind, ErrorKind::Limit);
    let events = decode(json!({"type":"response.output_item.done","item":{"type":"reasoning","summary":[],"encrypted_content":"opaque","unrelated":"not model context"}})).unwrap();
    let ModelEvent::Continuation { data, .. } = &events[0] else {
        panic!("expected continuation")
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(data).unwrap(),
        json!({"type":"reasoning","summary":[],"encrypted_content":"opaque"})
    );
}

#[test]
fn quota_events_keep_subscription_metadata_out_of_text_content() {
    let event = decode(json!({"type":"codex.rate_limits","rate_limits":{"primary":{"used_percent":42.25,"window_minutes":300,"reset_at":1700000000}},"credits":{"has_credits":true,"unlimited":false,"balance":"12.5"}})).unwrap();
    assert_eq!(
        event,
        vec![ModelEvent::Quota(antex_core::Quota {
            primary: Some(antex_core::QuotaWindow {
                used_basis_points: 4225,
                window_seconds: Some(18000),
                resets_at: Some(1700000000)
            }),
            secondary: None,
            credits: Some(antex_core::Credits {
                available: true,
                unlimited: false,
                balance: Some("12.5".into())
            }),
        })]
    );
}
