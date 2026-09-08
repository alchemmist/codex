# Antex extension protocol v1

This crate is a protocol scaffold, not a working extension host. The lifecycle,
process isolation, invocation-origin checks, and restart behavior described below
are requirements for the host that remains to be implemented.

Extensions exchange one JSON-RPC 2.0 object per newline-delimited UTF-8 frame.
The envelope follows the [JSON-RPC specification](https://www.jsonrpc.org/specification).
This transport profile uses string request IDs, named parameters, no batches,
and a 256 KiB frame limit. Standard output contains protocol frames only;
standard error contains bounded private diagnostic logs.

The host calls `initialize`, `tool/call`, `command/run`, `event/notify`, and
`shutdown`. `event/notify` is a request with an ID, not an ID-less JSON-RPC
notification: its response can publish status, session records, or the next
step of an explicitly started workflow. Initialization carries protocol and
Antex versions, session ID, native cwd, actual capability grants, and the
extension's own saved state. It never carries provider credentials or the
complete application configuration.

Every callable must declare all capabilities applied to its process, plus
any additional host-mediated capabilities it needs. Results contain bounded
text, optional one-line status/text panels, extension-owned records, and
declarative actions. The host checks each action before execution; an agent
action additionally requires an explicit user-command invocation. Ordinary
lifecycle events and model tool calls cannot start subagents.

Malformed frames, mismatched response IDs, incompatible versions, unknown
fields, oversized payloads, and undeclared permissions are errors. A failed
stateful invocation is never automatically replayed after restarting a process.
