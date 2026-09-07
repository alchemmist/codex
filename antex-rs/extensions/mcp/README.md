# Antex MCP extension

Each configured instance bridges one stdio or Streamable HTTP MCP server into Antex. Place the
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

Streamable HTTP accepts JSON and SSE responses, preserves `Mcp-Session-Id`, and
sends the negotiated `MCP-Protocol-Version` on subsequent requests:

```json
{
  "name": "docs",
  "program": "antex_ext_mcp.py",
  "arguments": ["--name", "docs", "--url", "https://example.com/mcp"],
  "capabilities": ["network"]
}
```

For a pre-issued bearer token, store it as a private `token` file beside the
extension and add `"--bearer-token-file", "token"`. Tokens are sent only in the
Authorization header and never in URLs or initialization payloads. Interactive
OAuth discovery and PKCE authorization remain to be implemented.
