# Antex MCP extension

Each configured instance bridges one stdio MCP server into Antex. Place the
executable and an `extension.json` in a directory under
`~/.antex/extensions/`. The extension process and the MCP child run inside the
same capability sandbox.

Example definition:

```json
{
  "name": "docs",
  "program": "antex_ext_mcp.py",
  "arguments": ["--name", "docs", "--", "docs-mcp-server"],
  "capabilities": ["shell"]
}
```

Streamable HTTP and OAuth transports are not handled by this entry point.
