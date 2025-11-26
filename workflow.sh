export RUSTFLAGS="-D warnings"
export RUSTDOCFLAGS="-D warnings"

cargo build --workspace --all-targets
cargo build --workspace --all-targets --all-features
cargo test --workspace --all-targets --all-features
cargo doc --examples --all-features --no-deps
cargo clippy --workspace --all-targets --all-features
cargo fmt --all -- --check