#!/usr/bin/env bash
# Build every deployable contract crate to wasm32-unknown-unknown and print
# the SHA-256 hash of each resulting artifact.
#
# Expects the workspace restructuring described in
# docs/deployment_guide.md#1-prerequisite-crate-structure: one crate per
# deployable contract under contracts/<name>/, each with
# `crate-type = ["cdylib", "rlib"]` and no native-only dependencies
# (tokio, axum, async-graphql, ...).
#
# Usage: ./scripts/build.sh [--release|--debug]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PROFILE="release"
if [[ "${1:-}" == "--debug" ]]; then
  PROFILE="debug"
fi

if [[ ! -d "contracts" ]]; then
  echo "error: contracts/ directory not found." >&2
  echo "" >&2
  echo "This repository is not yet restructured into a per-contract cdylib" >&2
  echo "workspace. See docs/deployment_guide.md section 1 (\"Prerequisite:" >&2
  echo "crate structure\") for what's required before this script can run." >&2
  exit 1
fi

if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
  echo "error: wasm32-unknown-unknown target not installed." >&2
  echo "  fix: rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi

if ! command -v soroban >/dev/null 2>&1; then
  echo "error: soroban CLI not found." >&2
  echo "  fix: cargo install --locked soroban-cli" >&2
  exit 1
fi

mkdir -p deployments
HASH_FILE="deployments/build_hashes.json"
echo "{" > "$HASH_FILE"

first=true
for crate_dir in contracts/*/; do
  name="$(basename "$crate_dir")"

  if ! grep -q 'crate-type.*cdylib' "$crate_dir/Cargo.toml" 2>/dev/null; then
    echo "warning: contracts/$name has no cdylib crate-type, skipping" >&2
    continue
  fi

  echo "==> building $name"
  (cd "$crate_dir" && cargo build --target wasm32-unknown-unknown --profile "$PROFILE")

  wasm_path="target/wasm32-unknown-unknown/$PROFILE/${name//-/_}.wasm"
  if [[ ! -f "$wasm_path" ]]; then
    echo "error: expected artifact not found at $wasm_path" >&2
    echo "  check that contracts/$name's [lib] name matches its crate directory name" >&2
    exit 1
  fi

  hash="$(sha256sum "$wasm_path" | awk '{print $1}')"
  echo "    $wasm_path"
  echo "    sha256: $hash"

  if [[ "$first" == true ]]; then
    first=false
  else
    echo "," >> "$HASH_FILE"
  fi
  printf '  "%s": {"path": "%s", "sha256": "%s"}' "$name" "$wasm_path" "$hash" >> "$HASH_FILE"
done

echo "" >> "$HASH_FILE"
echo "}" >> "$HASH_FILE"

echo ""
echo "Build hashes written to $HASH_FILE"
echo "Record these hashes in any upgrade proposal that references this build"
echo "(see docs/templates/upgrade_proposal_template.md section 3.2)."
