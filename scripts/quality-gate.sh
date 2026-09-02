#!/usr/bin/env bash

set -euo pipefail

echo "==> rustfmt"
cargo fmt --check

echo "==> clippy"
cargo clippy --all-targets --all-features -- -D warnings

echo "==> tests"
cargo test --all

echo "==> release build"
cargo build --release

echo
echo "QUALITY GATE PASSED"
