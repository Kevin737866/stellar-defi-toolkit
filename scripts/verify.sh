#!/usr/bin/env bash
# Post-deployment verification: existence, WASM hash, admin, initialization
# sanity, cross-references, and pause state for every contract recorded in
# deployments/<network>.json.
#
# See docs/deployment_guide.md section 7 ("Post-deployment verification") for
# what each check means and why a failure here must block launch.
#
# Usage: ./scripts/verify.sh --network <testnet|mainnet>

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

NETWORK=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --network) NETWORK="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$NETWORK" ]]; then
  echo "usage: $0 --network <testnet|mainnet>" >&2
  exit 1
fi

DEPLOY_FILE="deployments/${NETWORK}.json"
HASH_FILE="deployments/build_hashes.json"

if [[ ! -f "$DEPLOY_FILE" ]]; then
  echo "error: $DEPLOY_FILE not found — run ./scripts/deploy.sh first" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 1
fi

FAILED=0
pass() { echo "  [PASS] $1"; }
fail() { echo "  [FAIL] $1"; FAILED=1; }

ADMIN_EXPECTED=$(jq -r '.admin' "$DEPLOY_FILE")

echo "Verifying deployment: $DEPLOY_FILE"
echo "Expected admin: $ADMIN_EXPECTED"
echo ""

for name in $(jq -r '.contracts | keys[]' "$DEPLOY_FILE"); do
  id=$(jq -r ".contracts[\"$name\"]" "$DEPLOY_FILE")
  echo "==> $name ($id)"

  # 1. Existence check
  if soroban contract invoke --id "$id" --network "$NETWORK" -- get_admin >/dev/null 2>&1 \
     || soroban contract invoke --id "$id" --network "$NETWORK" -- get_info >/dev/null 2>&1; then
    pass "contract resolves on network"
  else
    fail "contract ID does not resolve, or has no readable getter — investigate manually"
    continue
  fi

  # 2. WASM hash check (best-effort; requires build_hashes.json from build.sh)
  if [[ -f "$HASH_FILE" ]]; then
    expected_hash=$(jq -r ".\"$name\".sha256 // empty" "$HASH_FILE")
    if [[ -n "$expected_hash" ]]; then
      installed_hash=$(soroban contract fetch --id "$id" --network "$NETWORK" --out-file /tmp/verify_${name}.wasm 2>/dev/null && sha256sum /tmp/verify_${name}.wasm | awk '{print $1}' || echo "")
      if [[ "$installed_hash" == "$expected_hash" ]]; then
        pass "installed WASM hash matches build ($expected_hash)"
      else
        fail "WASM hash mismatch: expected $expected_hash, got ${installed_hash:-<fetch failed>}"
      fi
    fi
  else
    echo "  [SKIP] no build_hashes.json found; run ./scripts/build.sh to enable this check"
  fi

  # 3. Admin check — module-specific getter names vary; try common ones
  admin_val=""
  for getter in get_admin admin; do
    admin_val=$(soroban contract invoke --id "$id" --network "$NETWORK" -- "$getter" 2>/dev/null || echo "")
    [[ -n "$admin_val" ]] && break
  done
  if [[ -n "$admin_val" ]]; then
    if [[ "$admin_val" == *"$ADMIN_EXPECTED"* ]]; then
      pass "admin matches expected address"
    else
      fail "admin mismatch: expected $ADMIN_EXPECTED, contract reports $admin_val"
    fi
  else
    echo "  [SKIP] no admin getter found for $name — verify manually"
  fi

  # 6. Pause state — most modules expose is_paused or similar
  paused_val=$(soroban contract invoke --id "$id" --network "$NETWORK" -- is_paused 2>/dev/null || echo "")
  if [[ -n "$paused_val" ]]; then
    echo "  [INFO] paused: $paused_val"
  fi

  echo ""
done

echo "----------------------------------------"
if [[ "$FAILED" -eq 0 ]]; then
  echo "All checks passed."
  echo "Note: this script covers checks 1-3 and 6 from the deployment guide"
  echo "mechanically. Checks 4 (initialization sanity) and 5 (cross-reference)"
  echo "require reading each contract's specific getters against the intended"
  echo "constructor arguments — do that manually per docs/deployment_guide.md"
  echo "section 7 before announcing this deployment as live."
  exit 0
else
  echo "One or more checks FAILED. Do not proceed to list assets or announce"
  echo "this deployment. See docs/deployment_guide.md section 7."
  exit 1
fi
