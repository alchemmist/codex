use std::collections::BTreeSet;

use antex_extension_protocol::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn response_envelopes_distinguish_null_results_from_missing_results() {
    let response: Response = decode(br#"{"jsonrpc":"2.0","id":"1","result":null}"#).unwrap();
    assert_eq!(response.result("1").unwrap(), serde_json::Value::Null);
    for invalid in [
        br#"{"jsonrpc":"2.0","id":"1"}"#.as_slice(),
        br#"{"jsonrpc":"2.0","id":"1","result":null,"error":{"code":1,"message":"bad"}}"#
            .as_slice(),
        br#"{"jsonrpc":"1.0","id":"1","result":{}}"#.as_slice(),
    ] {
        assert!(decode::<Response>(invalid).is_err());
    }
    let response: Response = decode(br#"{"jsonrpc":"2.0","id":"other","result":{}}"#).unwrap();
    assert!(matches!(
        response.result("1"),
        Err(ProtocolError::MismatchedId)
    ));
}

#[test]
fn manifest_permissions_cannot_understate_the_process_grants() {
    let mut manifest = Manifest {
        protocol_version: PROTOCOL_VERSION,
        name: "fixture".into(),
        tools: vec![Tool {
            name: "echo".into(),
            description: "Echo".into(),
            parameters: json!({"type":"object"}),
            permissions: BTreeSet::new(),
        }],
        commands: Vec::new(),
        events: BTreeSet::new(),
    };
    let grants = BTreeSet::from([Capability::Network]);
    assert!(manifest.validate("fixture", &grants).is_err());
    manifest.tools[0].permissions = grants.clone();
    manifest.validate("fixture", &grants).unwrap();
    manifest.protocol_version += 1;
    assert!(matches!(
        manifest.validate("fixture", &grants),
        Err(ProtocolError::Version)
    ));
}

#[test]
fn publications_and_privileged_actions_require_explicit_capabilities() {
    let output = Output {
        text: "done".into(),
        records: vec![json!({"step":1})],
        status: Some("ready".into()),
        panel: None,
        actions: vec![Action::Agent {
            id: "worker".into(),
            prompt: "inspect".into(),
            model: None,
        }],
    };
    assert!(output.validate(&BTreeSet::new()).is_err());
    output
        .validate(&BTreeSet::from([
            Capability::Persist,
            Capability::Ui,
            Capability::Agent,
        ]))
        .unwrap();
    let oversized = Output {
        text: "x".repeat(MAX_TEXT_BYTES + 1),
        ..Output::default()
    };
    assert!(oversized.validate(&BTreeSet::new()).is_err());
}

#[test]
fn frames_are_bounded_before_deserialization() {
    assert!(matches!(
        decode::<Response>(&vec![b' '; MAX_FRAME_BYTES + 1]),
        Err(ProtocolError::Limit("frame"))
    ));
    let request = Request {
        jsonrpc: Version::V2,
        id: "1".into(),
        method: Method::Shutdown,
        params: json!({}),
    };
    let encoded = encode(&request).unwrap();
    let decoded: Request = decode(&encoded).unwrap();
    assert_eq!(decoded.id, request.id);
}
