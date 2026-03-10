#!/bin/sh
fail() {
    echo "✗ $1 failed. Push aborted." >&2
    exit 1
}

echo "→ Formatting..."
cargo fmt --check || fail "Formatting"

echo "→ Running Clippy..."
cargo clippy -- -D warnings || fail "Clippy"

echo "→ Running tests..."
cargo test || fail "Tests"

echo "→ Auditing dependencies..."
cargo audit || fail "Audit"

echo "→ Looking for unused dependencies..."
cargo machete || fail "Machete"

echo "→ Preparing queries"
sqlx prepare -- --all-targets || fail "Query Prep"

echo "✓ Pre-push checks passed."