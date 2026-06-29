# Liquidation CLI Commands

This document describes the new CLI commands added to the Stellar DeFi Toolkit for managing liquidations of undercollateralized positions.

## Overview

The liquidation feature allows liquidators to identify and liquidate undercollateralized positions in the lending protocol. When a borrower's position falls below the required collateralization ratio (health factor < 1.0), liquidators can repay part of the debt and seize collateral at a discount.

## Commands

### 1. `liquidate` - Execute a Liquidation

Liquidate an undercollateralized position in the lending protocol.

#### Usage

```bash
stellar-defi-cli liquidate \
  --liquidator <LIQUIDATOR_ADDRESS> \
  --borrower <BORROWER_ADDRESS> \
  --debt-asset <DEBT_ASSET_SYMBOL> \
  --collateral-asset <COLLATERAL_ASSET_SYMBOL> \
  --repay-amount <AMOUNT> \
  [--debt-price <PRICE>] \
  [--collateral-price <PRICE>] \
  [--timestamp <UNIX_TIMESTAMP>] \
  [--dry-run]
```

#### Parameters

- `--liquidator` (required): Address of the liquidator executing the liquidation
- `--borrower` (required): Address of the borrower whose position will be liquidated
- `--debt-asset` (required): Symbol of the debt asset (e.g., "USDC")
- `--collateral-asset` (required): Symbol of the collateral asset (e.g., "XLM")
- `--repay-amount` (required): Amount of debt to repay (in smallest unit, with 18 decimals)
- `--debt-price` (optional): Price of debt asset in USD with 18 decimals (default: 1.0 = 1000000000000000000)
- `--collateral-price` (optional): Price of collateral asset in USD with 18 decimals (default: 1.0 = 1000000000000000000)
- `--timestamp` (optional): Unix timestamp for the liquidation (default: current time)
- `--dry-run` (optional): Simulate the liquidation without executing it

#### Examples

**Example 1: Dry-run liquidation simulation**

```bash
stellar-defi-cli liquidate \
  --liquidator "GCLIQUIDATOR123" \
  --borrower "GCBORROWER456" \
  --debt-asset "USDC" \
  --collateral-asset "XLM" \
  --repay-amount 1000000000000000000000 \
  --debt-price 1000000000000000000 \
  --collateral-price 500000000000000000 \
  --dry-run
```

This simulates liquidating a position where:
- Repaying 1000 USDC (1000 * 10^18)
- USDC price: $1.00
- XLM price: $0.50
- The liquidator would receive collateral worth more than the repaid debt due to the liquidation bonus

**Example 2: Execute actual liquidation**

```bash
stellar-defi-cli liquidate \
  --liquidator "GCLIQUIDATOR123" \
  --borrower "GCBORROWER456" \
  --debt-asset "USDC" \
  --collateral-asset "XLM" \
  --repay-amount 500000000000000000000 \
  --debt-price 1000000000000000000 \
  --collateral-price 800000000000000000
```

This executes a liquidation repaying 500 USDC at current prices.

#### Output

The command provides detailed output including:
- Liquidation request details
- Position snapshot (collateral value, debt value, health factor)
- Liquidation calculation (repay value, liquidation bonus, seize amount)
- Liquidation result (repaid amount, seized collateral, liquidator profit)

**Dry-run output example:**
```
🔍 Liquidation Request
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Liquidator:        GCLIQUIDATOR123
Borrower:          GCBORROWER456
Debt Asset:        USDC
Collateral Asset:  XLM
Repay Amount:      1000000000000000000000
Debt Price:        $1.000000
Collateral Price:  $0.500000
Dry Run:           Yes
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🔬 DRY RUN MODE - Simulating liquidation...

📊 Position Snapshot:
   Collateral Value:   $1500.00
   Liquidation Value:  $1200.00
   Debt Value:         $1300.00
   Health Factor:      0.9231

✅ Position IS liquidatable (health factor < 1.0)
   The position is undercollateralized and can be liquidated.

💰 Liquidation Calculation:
   Repay Value:        $1000.00
   Liquidation Bonus:  10.00%
   Discounted Value:   $1100.00
   Seize Amount:       2200.000000 XLM
   Liquidator Profit:  $100.00
```

### 2. `check-liquidation` - Check Position Status

Check if a borrower's position is liquidatable without executing a liquidation.

#### Usage

```bash
stellar-defi-cli check-liquidation \
  --borrower <BORROWER_ADDRESS> \
  --debt-asset <DEBT_ASSET_SYMBOL> \
  --collateral-asset <COLLATERAL_ASSET_SYMBOL> \
  [--debt-price <PRICE>] \
  [--collateral-price <PRICE>]
```

#### Parameters

- `--borrower` (required): Address of the borrower to check
- `--debt-asset` (required): Symbol of the debt asset (e.g., "USDC")
- `--collateral-asset` (required): Symbol of the collateral asset (e.g., "XLM")
- `--debt-price` (optional): Price of debt asset in USD with 18 decimals (default: 1.0)
- `--collateral-price` (optional): Price of collateral asset in USD with 18 decimals (default: 1.0)

#### Examples

**Example: Check if a position is liquidatable**

```bash
stellar-defi-cli check-liquidation \
  --borrower "GCBORROWER456" \
  --debt-asset "USDC" \
  --collateral-asset "XLM" \
  --debt-price 1000000000000000000 \
  --collateral-price 500000000000000000
```

#### Output

The command provides:
- Position details (supplied assets, debt assets)
- Position values (collateral value, liquidation value, debt value)
- Health factor
- Status (HEALTHY, LIQUIDATABLE, or NO DEBT)
- Safety buffer or collateral deficit percentage

**Healthy position output example:**
```
🔍 Checking Liquidation Status
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Borrower:          GCBORROWER456
Debt Asset:        USDC
Collateral Asset:  XLM
Debt Price:        $1.000000
Collateral Price:  $0.500000
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📊 Position Details:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

💰 Supplied Assets:
   XLM: 5000.000000

📉 Debt Assets:
   USDC: 1000.000000

💵 Position Values:
   Collateral Value:   $2500.00
   Liquidation Value:  $2000.00
   Debt Value:         $1000.00

🏥 Health Factor: 2.0000

✅ Status: HEALTHY
   The position is well-collateralized and cannot be liquidated.
   Safety Buffer: 100.00%
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Liquidatable position output example:**
```
🔍 Checking Liquidation Status
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Borrower:          GCBORROWER456
Debt Asset:        USDC
Collateral Asset:  XLM
Debt Price:        $1.000000
Collateral Price:  $0.500000
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📊 Position Details:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

💰 Supplied Assets:
   XLM: 2000.000000

📉 Debt Assets:
   USDC: 1000.000000

💵 Position Values:
   Collateral Value:   $1000.00
   Liquidation Value:  $800.00
   Debt Value:         $1000.00

🏥 Health Factor: 0.8000

⚠️  Status: LIQUIDATABLE
   The position is undercollateralized and can be liquidated!
   Collateral Deficit: 20.00%

💡 Liquidation Opportunity:
   You can liquidate this position to earn a liquidation bonus.
   Use the 'liquidate' command to execute the liquidation.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

## Understanding Key Concepts

### Health Factor

The health factor is a measure of a position's collateralization:
- **Health Factor > 1.0**: Position is healthy and cannot be liquidated
- **Health Factor < 1.0**: Position is undercollateralized and can be liquidated
- **Health Factor = 0**: Position has no debt

Formula: `Health Factor = Liquidation Value / Debt Value`

### Liquidation Bonus

When liquidating a position, the liquidator receives a bonus (typically 5-10%) on top of the repaid debt value. This incentivizes liquidators to maintain protocol solvency.

Example:
- Liquidator repays $1000 USDC
- Liquidation bonus: 10%
- Liquidator receives collateral worth $1100
- Liquidator profit: $100

### Close Factor

The close factor limits how much of a position can be liquidated in a single transaction (typically 50%). This prevents liquidators from liquidating entire positions at once.

## Price Formatting

All prices use 18 decimal places (WAD format):
- $1.00 = 1000000000000000000
- $0.50 = 500000000000000000
- $100.00 = 100000000000000000000

To convert from decimal to WAD: `price * 10^18`
To convert from WAD to decimal: `price / 10^18`

## Error Handling

The CLI provides helpful error messages for common issues:

- **PositionNotLiquidatable**: The position's health factor is >= 1.0
- **InsufficientBalance**: The borrower doesn't have enough collateral
- **InsufficientLiquidity**: The protocol doesn't have enough liquidity
- **UnknownAsset**: The specified asset is not registered
- **InvalidAmount**: The amount must be positive

## Integration with Stellar

This CLI tool is designed to work with Soroban smart contracts on the Stellar network. In a production environment:

1. Connect to a Stellar node
2. Load actual position data from the blockchain
3. Use real oracle prices
4. Sign and submit transactions to the network

## Development and Testing

For development and testing:

1. Use `--dry-run` to simulate liquidations without executing them
2. Test with various price scenarios to understand liquidation mechanics
3. Monitor health factors to identify liquidation opportunities
4. Verify calculations before executing real liquidations

## Next Steps

To integrate this CLI with a live Stellar network:

1. Add Stellar SDK integration for transaction signing
2. Connect to price oracles for real-time price feeds
3. Implement position monitoring and alerting
4. Add support for multiple collateral types
5. Create automated liquidation bots

## Support

For issues or questions:
- Check the main README.md
- Review the lending protocol documentation in `docs/`
- Examine the source code in `src/contracts/lending.rs`
