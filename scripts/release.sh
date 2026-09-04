#!/usr/bin/env bash
# Fail-closed release script for Linux and macOS.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="$(awk -F '"' '/^version = "/ { print $2; exit }' Cargo.toml)"
if [[ -z "$version" ]]; then
    echo "Could not determine the package version from Cargo.toml" >&2
    exit 1
fi
tag="v${version}"

echo "=== Neo Decompiler ${tag} Release ==="

branch="$(git branch --show-current)"
if [[ "$branch" != "master" ]]; then
    echo "Releases must run from master; current branch is '${branch}'" >&2
    exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
    echo "Release requires a clean working tree" >&2
    exit 1
fi

git fetch --quiet origin master --tags
release_head="$(git rev-parse HEAD)"
if [[ "$release_head" != "$(git rev-parse origin/master)" ]]; then
    echo "HEAD must exactly match origin/master before release" >&2
    exit 1
fi
existing_tag="$(git tag --list "$tag")"
if [[ -n "$existing_tag" ]]; then
    echo "Tag ${tag} already exists" >&2
    exit 1
fi

echo "Step 1: Checking formatting..."
cargo fmt --all -- --check

echo "Step 2: Running clippy..."
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings

echo "Step 3: Running tests..."
cargo test --locked --all-features
cargo test --locked --no-default-features

echo "Step 4: Running benchmarks..."
cargo bench --locked

echo "Step 5: Building release..."
cargo build --locked --release

echo "Step 6: Generating docs..."
RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -D warnings" cargo doc --locked --no-deps

echo "Step 7: Checking the JavaScript packages..."
npm --prefix js test
npm --prefix web ci --ignore-scripts
npm --prefix web run version:check
npm --prefix web test
npm --prefix web run verify:pack
npm --prefix web run test:wasm

echo "Step 8: Checking the crates.io package..."
cargo package --locked
cargo publish --locked --dry-run

echo "=== All checks passed for ${tag}. ==="
read -r -p "Publish neo-decompiler ${version} to crates.io? (y/n) " reply
if [[ "$reply" =~ ^[Yy]$ ]]; then
    if [[ "$(git rev-parse HEAD)" != "$release_head" || -n "$(git status --porcelain)" ]]; then
        echo "Checkout changed during release checks; rerun them before publishing" >&2
        exit 1
    fi
    cargo publish --locked
    git tag -s "$tag" "$release_head" -m "neo-decompiler ${tag}"
    git push origin "$tag"
    echo "=== Published and pushed ${tag} successfully. ==="
else
    echo "Skipped publishing. After review, run:"
    echo "  cargo publish --locked"
    echo "  git tag -s ${tag} -m 'neo-decompiler ${tag}'"
    echo "  git push origin ${tag}"
fi
