# Flash Loan Functionality

Flash loans are a powerful DeFi primitive that allows users to borrow assets from a pool without providing collateral, provided the total borrowed amount (plus a small fee) is returned within the same atomic transaction.

## Overview

The `FlashLoanContract` in the Stellar DeFi Toolkit provides a secure and efficient way to implement flash loans on the Soroban smart contract platform.

### Key Features

- **Single-Transaction Borrow/Repay**: Automatic verification that funds are returned.
- **Low Fees**: 0.09% fee (9 basis points) by default.
- **Reentrancy Protection**: Built-in safeguards to prevent malicious recursive calls.
- **Arbitrage Detection**: Safety limits on loan sizes to mitigate price manipulation risks.
- **Liquidation Helpers**: Pre-built logic for common liquidation use cases.

## Usage

To use flash loans, your contract must follow the callback pattern.

### 1. Requesting a Flash Loan

Call the `flash_loan` function on the `FlashLoanContract`:

```rust
let fee = flash_loan_contract.flash_loan(
    env,
    my_contract_address,
    borrow_amount,
    arbitrary_params
)?;
```

### 2. Implementing the Callback

Your contract must implement an `on_flash_loan` function that will be invoked by the flash loan contract:

```rust
#[contractimpl]
impl MyContract {
    pub fn on_flash_loan(
        env: Env,
        initiator: Address,
        amount: u64,
        fee: u64,
        params: Bytes,
    ) -> Result<(), MyError> {
        // 1. Perform your logic (arbitrage, liquidation, etc.)
        // 2. Ensure your contract has (amount + fee) tokens to repay the loan
        // 3. The flash loan contract will automatically check balance after this returns
        Ok(())
    }
}
```

## Security Considerations

- **Atomicity**: The entire transaction fails if the loan is not repaid, ensuring the pool is never left at a loss.
- **Access Control**: Admin functions (like changing fees or pausing) are protected.
- **Input Validation**: Zero-amount loans are rejected, and large loans trigger arbitrage safeguards.

## Examples

### Arbitrage Example

1. Take flash loan of USDC.
2. Swap USDC for XLM on DEX A.
3. Swap XLM for USDC on DEX B (where price is higher).
4. Repay USDC loan + fee.
5. Keep the profit.

### Liquidation Example

1. Take flash loan of the debt token.
2. Repay a user's underwater debt in a lending protocol.
3. Seize their collateral tokens.
4. Swap collateral for the debt token to repay the flash loan.
5. Keep the liquidation bonus.
