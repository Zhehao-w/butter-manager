#!/usr/bin/env bash
# Linux frontend/core checks; Windows CI owns native checks and the release EXE.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

if [[ $(uname -s) != Linux ]]; then
  echo 'Run this entry point on Linux; see CONTRIBUTING.md for Windows checks.' >&2
  exit 1
fi

expected_node=$(tr -d '\r\n' < .node-version)
expected_pnpm=$(node -p "require('./package.json').packageManager.split('@')[1]")
[[ $(node --version) == "v$expected_node" ]] || { echo "Node.js $expected_node is required." >&2; exit 1; }
[[ $(pnpm --version) == "$expected_pnpm" ]] || { echo "pnpm $expected_pnpm is required." >&2; exit 1; }
# rustup reads the pinned rust-toolchain.toml; the environment must already have rustup.
cargo --version

export CARGO_HOME="$PWD/.tools/cargo-cache"
export TMPDIR="$PWD/.tools/tmp"
export PNPM_CONFIG_STATE_DIR="$PWD/.tools/pnpm-state"
mkdir -p "$CARGO_HOME" "$TMPDIR" "$PNPM_CONFIG_STATE_DIR"

pnpm install --frozen-lockfile
pnpm format:check
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
pnpm test:ui
pnpm build
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --lib
