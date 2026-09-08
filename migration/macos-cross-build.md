# macOS build on deimos

The Apple Silicon release binary was built successfully on Linux using Rust
1.95.0, Clang 17, Rust's bundled LLD and the maintainer's MacOSX 26.5 SDK.
The SDK is copied privately from the maintainer's Xcode installation to
`/home/antonmoss/antex-tools/apple-sdk/MacOSX.sdk`; it is not a release artifact.
No Rust compilation is performed on the Mac.

Synchronize `Makefile`, `justfile` and `antex-rs/` before building. Synchronizing
only the Rust directory leaves obsolete root build commands in an old checkout.
`scripts/antex-remote.py` stamps the local source commit into remote builds,
including `.dirty` when the Rust workspace has uncommitted changes.

```sh
python3 scripts/antex-remote.py \
  --host deimos.vla.yp-c.yandex.net \
  --checkout /home/antonmoss/antex-work/validation-host \
  --tools /home/antonmoss/antex-tools --jobs 4 \
  env CARGO_BUILD_TARGET=aarch64-apple-darwin \
  SDKROOT=/home/antonmoss/antex-tools/apple-sdk/MacOSX.sdk \
  MACOSX_DEPLOYMENT_TARGET=11.0 \
  CC_aarch64_apple_darwin=clang-17 \
  AR_aarch64_apple_darwin=llvm-ar-17 \
  'CFLAGS_aarch64_apple_darwin=--target=arm64-apple-darwin -isysroot /home/antonmoss/antex-tools/apple-sdk/MacOSX.sdk' \
  CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/home/antonmoss/antex-tools/rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld \
  'CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS=-C linker-flavor=ld64.lld -C link-arg=-syslibroot -C link-arg=/home/antonmoss/antex-tools/apple-sdk/MacOSX.sdk' \
  make -C .. build
```

Download `antex-rs/target/aarch64-apple-darwin/release/antex` to the ignored
`dist/antex-macos-arm64/antex`. Compare SHA-256 with the remote artifact before
running `codesign --force --sign -` locally, then verify the signature and run
`--version`. Ad-hoc signing changes the checksum.

The first cross-build completed in 2m09s and produced a 15,875,248-byte Mach-O
arm64 executable. Local signature verification, startup and `/status` passed.
The TUI remained usable at 100x30 and 32x16. An idle process measured 19,376 KiB
RSS. This is an observed sample, not the full startup/memory acceptance campaign.

Local smoke uses a separate home under `dist/antex-macos-arm64/home`; it does not
import credentials from deimos or the installed Codex fallback. Release/platform
gates remain open until all required interactive and installer checks pass.
