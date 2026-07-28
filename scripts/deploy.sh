#!/usr/bin/env bash
# Deploy and initialise the toolkit's Soroban contracts in dependency order.
#
# Order follows docs/deployment_guide.md section 5 ("Contract initialisation
# order"): tokens -> oracle infrastructure -> circuit breaker -> stablecoin ->
# stablecoin dependents -> synthetic protocol -> standalone modules.
#
# NOT idempotent: re-running deploys new instances. To change a live
# deployment, use the upgrade process in docs/upgrade_governance_process.md
# instead of re-running this script.
#
# Usage:
#   ./scripts/deploy.sh --network testnet
#   ./scripts/deploy.sh --network mainnet --confirm

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

NETWORK=""
CONFIRMED=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --network) NETWORK="$2"; shift 2 ;;
    --confirm) CONFIRMED=true; shift ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$NETWORK" ]]; then
  echo "usage: $0 --network <testnet|mainnet> [--confirm]" >&2
  exit 1
fi

if [[ ! -f ".env" ]]; then
  echo "error: .env not found. Copy .env.example to .env and fill it in first." >&2
  exit 1
fi
# shellcheck disable=SC1091
source .env

if [[ "$NETWORK" == "mainnet" && "$CONFIRMED" != true ]]; then
  echo "Mainnet deployment requires --confirm."
  echo ""
  echo "Before re-running with --confirm, verify:"
  echo "  - Admin address below is a MULTISIG, not a single key"
  echo "  - .env points at mainnet RPC, not testnet (check SOROBAN_RPC_URL)"
  echo "  - The build hashes in deployments/build_hashes.json match what was audited"
  echo "  - See docs/deployment_guide.md section 9 (\"Mainnet readiness gate\")"
  echo ""
  echo "Deploy plan for network=$NETWORK, admin=${ADMIN_ADDRESS:-<unset>}:"
  echo "  1. Token contracts"
  echo "  2. Oracle infrastructure (asset_registry, multi_asset_oracle, price_oracle,"
  echo "     oracle_manager, decentralized_oracle)"
  echo "  3. circuit_breaker"
  echo "  4. stablecoin"
  echo "  5. stability_pool, arbitrage, governance_v2 (depend on stablecoin)"
  echo "  6. synthetic_protocol, synthetic_governance"
  echo "  7. staking, flash_loan, liquidity_pool, position_manager, price_feed_adapters"
  exit 1
fi

if [[ -z "${ADMIN_ADDRESS:-}" ]]; then
  echo "error: ADMIN_ADDRESS not set in .env" >&2
  echo "  mainnet: this must be a multisig address, not a deployer key" >&2
  exit 1
fi

if [[ ! -d "contracts" ]]; then
  echo "error: contracts/ directory not found — see docs/deployment_guide.md section 1." >&2
  exit 1
fi

mkdir -p deployments
OUT_FILE="deployments/${NETWORK}.json"
if [[ -f "$OUT_FILE" ]]; then
  echo "warning: $OUT_FILE already exists. This script does not update existing"
  echo "deployments. Move or rename it first if you intend to deploy fresh"
  echo "instances, or use the upgrade process instead."
  exit 1
fi

WASM_DIR="target/wasm32-unknown-unknown/release"

deploy_contract() {
  local name="$1"
  local wasm="${WASM_DIR}/${name}.wasm"
  if [[ ! -f "$wasm" ]]; then
    echo "error: $wasm not found — run ./scripts/build.sh first" >&2
    exit 1
  fi
  soroban contract deploy \
    --wasm "$wasm" \
    --source deployer \
    --network "$NETWORK" \
    --rpc-url "$SOROBAN_RPC_URL" \
    --network-passphrase "$SOROBAN_NETWORK_PASSPHRAPH"
}

echo "==> Stage 1: token contracts"
# Fill in the specific tokens this deployment needs (e.g. governance token,
# SUSD's own token if issued separately from the stablecoin contract).
# soroban_token_contract expects: initialize(admin, supply)

echo "==> Stage 2: oracle infrastructure"
ASSET_REGISTRY_ID=$(deploy_contract "asset_registry")
soroban contract invoke --id "$ASSET_REGISTRY_ID" --source deployer --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDRESS"

MULTI_ASSET_ORACLE_ID=$(deploy_contract "multi_asset_oracle")
soroban contract invoke --id "$MULTI_ASSET_ORACLE_ID" --source deployer --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDRESS" --asset_registry_address "$ASSET_REGISTRY_ID"

echo "==> Stage 3: circuit breaker"
CIRCUIT_BREAKER_ID=$(deploy_contract "circuit_breaker")
soroban contract invoke --id "$CIRCUIT_BREAKER_ID" --source deployer --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDRESS"

echo "==> Stage 4: stablecoin"
STABLECOIN_ID=$(deploy_contract "stablecoin")
soroban contract invoke --id "$STABLECOIN_ID" --source deployer --network "$NETWORK" \
  -- initialize \
  --admin "$ADMIN_ADDRESS" \
  --name "${STABLECOIN_NAME:-Stellar USD}" \
  --symbol "${STABLECOIN_SYMBOL:-SUSD}" \
  --oracle "$MULTI_ASSET_ORACLE_ID"

echo "==> Stage 5: stablecoin dependents"
STABILITY_POOL_ID=$(deploy_contract "stability_pool")
soroban contract invoke --id "$STABILITY_POOL_ID" --source deployer --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDRESS" --stablecoin_address "$STABLECOIN_ID" \
  --treasury_address "${TREASURY_ADDRESS:?TREASURY_ADDRESS must be set in .env}"

GOVERNANCE_ID=$(deploy_contract "governance_v2")
soroban contract invoke --id "$GOVERNANCE_ID" --source deployer --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDRESS" --stablecoin_address "$STABLECOIN_ID"

echo "==> Stage 6: synthetic protocol"
SYNTHETIC_ID=$(deploy_contract "synthetic_protocol")
soroban contract invoke --id "$SYNTHETIC_ID" --source deployer --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDRESS"
# synthetic_governance requires a governance token address deployed in Stage 1 —
# fill in GOVERNANCE_TOKEN_ID and uncomment:
# SYNTHETIC_GOV_ID=$(deploy_contract "synthetic_governance")
# soroban contract invoke --id "$SYNTHETIC_GOV_ID" --source deployer --network "$NETWORK" \
#   -- initialize --admin "$ADMIN_ADDRESS" --governance_token "$GOVERNANCE_TOKEN_ID"

echo "==> Stage 7: standalone modules"
# staking, flash_loan, liquidity_pool, position_manager, price_feed_adapters —
# each needs asset/token addresses specific to this deployment; fill in below
# as needed rather than deploying blind defaults.

cat > "$OUT_FILE" <<EOF
{
  "network": "$NETWORK",
  "admin": "$ADMIN_ADDRESS",
  "contracts": {
    "asset_registry": "$ASSET_REGISTRY_ID",
    "multi_asset_oracle": "$MULTI_ASSET_ORACLE_ID",
    "circuit_breaker": "$CIRCUIT_BREAKER_ID",
    "stablecoin": "$STABLECOIN_ID",
    "stability_pool": "$STABILITY_POOL_ID",
    "governance_v2": "$GOVERNANCE_ID",
    "synthetic_protocol": "$SYNTHETIC_ID"
  }
}
EOF

echo ""
echo "Deployment recorded in $OUT_FILE"
echo "Next: ./scripts/verify.sh --network $NETWORK"
