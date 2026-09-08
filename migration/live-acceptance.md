# Live acceptance on deimos

Date: 2026-09-08. These are explicit manual acceptance commands, separate from
automated tests, which use fabricated credentials. No token values are recorded.

- Device OAuth completed after the maintainer confirmed the browser code.
- Model discovery initially failed during TLS verification. The deimos network
  presents a certificate issued by YandexInternalCA; system curl trusted it while
  the provider's bundled WebPKI roots did not. The provider now uses system trust
  through reqwest's rustls-tls-native-roots feature. Certificate verification
  remains enabled. A local HTTPS fixture reproduced the failure before the fix;
  it now accepts an explicitly trusted fixture CA and rejects an untrusted CA.
- All 18 provider tests passed, followed by local formatting and remote scoped
  Clippy fix. Temporary diagnostic instrumentation was removed.
- Live model discovery returned seven visible models.
- A plain gpt-5.4-mini turn returned exactly ANTEX_LIVE_OK.
- A separate live task performed write, edit, read and shell in that order. The
  isolated workspace's acceptance.txt contains beta followed by a newline. The
  final response was ANTEX_TOOLS_OK.
- Resuming that session recalled beta without executing tools.
- Forking from the original user message returned ANTEX_FORK_OK without tools.
- A native Antex identification probe returned the same seven-model catalog.
  Responses transport under native identification is still unverified, so the
  working private compatibility contract remains in the downloaded build.

The acceptance state is isolated under
/home/antonmoss/antex-work/preflight-home. The workspace is
/home/antonmoss/antex-work/acceptance-OWPc1K. This state is not a release artifact.
The installed Codex fallback and its state were not modified.

Still required: live interruption, native Responses transport identification
experiment, interactive terminal/platform acceptance and all release gates.
