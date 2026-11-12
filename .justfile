set shell := ["bash", "-euo", "pipefail", "-c"]

ci:
    cargo fmt --all --check
    cargo check --lib --no-default-features
    cargo nextest run --lib --no-default-features
    cargo check --lib
    cargo check --all-features
    RUSTFLAGS='-Dmissing_docs' cargo check --lib --all-features
    RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
    cargo test --doc --all-features
    cargo check --lib --no-default-features --features blocking-tls-rustls-ring
    cargo nextest run --lib
    cargo clippy --all-targets --all-features -- -D warnings
    cargo clippy --all-targets --no-default-features -- -D warnings
    cargo clippy --all-targets --no-default-features --features blocking-tls-rustls-ring -- -D warnings

patch:
    cargo release patch --no-publish --execute

minor:
    cargo release minor --no-publish --execute

publish:
    cargo publish
