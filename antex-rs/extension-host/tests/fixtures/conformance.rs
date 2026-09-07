use std::io::BufRead;
use std::io::Write;

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let id = field(&line, "id").unwrap();
        let result = if line.contains("\"method\":\"initialize\"") {
            r#"{"protocolVersion":1,"name":"fixture","tools":[{"name":"echo","description":"Echo text","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false},"permissions":[]}],"commands":[{"name":"hello","description":"Say hello","permissions":[]}],"events":["turnComplete"]}"#.to_string()
        } else if line.contains("\"method\":\"tool/call\"") {
            let text = field(&line, "text").unwrap();
            format!(r#"{{"text":"{text}","records":[],"actions":[]}}"#)
        } else if line.contains("\"method\":\"command/run\"") {
            r#"{"text":"hello","records":[],"actions":[]}"#.to_string()
        } else {
            "null".to_string()
        };
        writeln!(
            stdout,
            r#"{{"jsonrpc":"2.0","id":"{id}","result":{result}}}"#
        )
        .unwrap();
        stdout.flush().unwrap();
        if line.contains("\"method\":\"shutdown\"") {
            break;
        }
    }
}

fn field(input: &str, name: &str) -> Option<String> {
    let marker = format!(r#""{name}":"#);
    let start = input.find(&marker)? + marker.len();
    let value = input.get(start..)?.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}
