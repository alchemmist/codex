set working-directory := "antex-rs"
set positional-arguments

export ANTEX_REPO_ROOT := justfile_directory()

antex *args:
    cargo run -p antex-cli -- {{args}}

fmt:
    cargo fmt

fmt-check:
    cargo fmt -- --check

fix *args:
    cargo clippy --fix --tests --allow-dirty {{args}}

clippy *args:
    cargo clippy --tests {{args}}

test *args:
    RUST_MIN_STACK=8388608 NEXTEST_PROFILE=local cargo nextest run --no-fail-fast {{args}}

bench *args:
    cargo bench {{args}}

bench-smoke *args:
    cargo bench {{args}} -- --test
