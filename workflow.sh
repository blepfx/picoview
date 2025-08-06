RUSTFLAGS="-D warnings"
RUSTDOCFLAGS="-D warnings"

cargo build --workspace --all-targets --verbose
cargo build --workspace --all-targets --all-features --verbose
cargo test --workspace --all-targets --all-features --verbose
cargo doc --examples --all-features --no-deps
cargo fmt --all -- --check