# Kernel context review

P0 manual-review item before cutover: `ContextFragment` and `ToolOutput` each
permit up to 8,000 UTF-8 bytes of text. These can exceed 1,000 model tokens. The
8,000-byte cap is conservative relative to the 10,000-token per-item ceiling,
but does not eliminate the required manual review of their injection sites.

Instruction fragments reject oversized input. Tool results preserve a valid UTF-8
prefix and include a truncation marker within the cap. Model requests must also
validate tool identifiers, schemas, aggregate history, image bytes, and opaque
continuation state before sampling. Provider-owned continuation state is opaque
to the kernel and must not be interpreted as instructions by other modules.

The kernel context module owns these fragments and their
`ContextualUserFragment` conversions. UI-only events must not be converted into
messages implicitly. The standalone kernel is not yet the active Antex runtime;
these checks and the agent-loop integration suite must pass before switching it in.
