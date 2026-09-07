# Antex Python extension SDK

Copy `antex_extension.py` next to an extension entry point, register tools and
commands on `Extension`, then call `run()`. The module uses only Python's standard
library and communicates over JSON-RPC 2.0 NDJSON on stdin/stdout.
