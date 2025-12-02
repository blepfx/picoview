export RUSTFLAGS="-Dwarnings -Zsanitizer=address"
export RUSTDOCFLAGS="-Dwarnings"

cargo build -Zbuild-std --workspace --all-targets
cargo build -Zbuild-std --workspace --all-targets --all-features
cargo test -Zbuild-std --workspace --all-targets --all-features
cargo doc --examples --all-features --no-deps
cargo clippy --workspace --all-targets --all-features
cargo fmt --all -- --check