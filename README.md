# Stellar DeFi Toolkit 🚀

A comprehensive DeFi toolkit for building decentralized finance applications on the Stellar blockchain using Soroban smart contracts.

## ✨ Features

- **🪙 Token Contracts**: Complete ERC-20-like token implementation on Stellar
- **💧 Liquidity Pools**: Automated market maker (AMM) liquidity pools
- **💰 Lending & Borrowing**: Collateralized lending protocol with liquidations
- **🌾 Yield Farming**: Staking and reward distribution mechanisms
- **🌉 Cross-chain Bridges**: Asset transfer between different blockchains
- **🏛️ Governance**: Decentralized governance and voting systems
- **📊 Analytics**: Real-time DeFi protocol analytics and monitoring
- **🛡️ Circuit Breakers**: Automatic protection against extreme price volatility
- **🔮 Oracle System**: Multi-source price aggregation with deviation detection
- **🛠️ Developer Tools**: CLI tools and SDK for easy development

## 🚀 Quick Start

### Prerequisites

- Rust 1.70.0 or higher
- Stellar CLI tools
- Soroban CLI

### Installation

#### From Crates.io (Coming Soon)

```bash
cargo install stellar-defi-toolkit
```

#### From Source

```bash
git clone https://github.com/yourusername/stellar-defi-toolkit.git
cd stellar-defi-toolkit
cargo build --release
```

## 📖 Usage

### CLI Usage

#### Quote Interest Rate

```bash
stellar-defi-cli quote-rate --utilization-bps 8000
```

#### Check if a Position is Liquidatable

```bash
stellar-defi-cli check-liquidation \
  --borrower "GCBORROWER456" \
  --debt-asset "USDC" \
  --collateral-asset "XLM" \
  --debt-price 1000000000000000000 \
  --collateral-price 500000000000000000
```

#### Liquidate an Undercollateralized Position

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

For detailed documentation on liquidation commands, see [CLI_LIQUIDATION.md](CLI_LIQUIDATION.md).

#### Deploy a New Token

```bash
stellar-defi-cli deploy-token \
  --name "My Token" \
  --symbol "MTK" \
  --supply 1000000
```

#### Create a Liquidity Pool

```bash
stellar-defi-cli create-pool \
  --token-a "TOKEN_A_CONTRACT_ID" \
  --token-b "TOKEN_B_CONTRACT_ID"
```

#### Stake Tokens

```bash
stellar-defi-cli stake \
  --contract-id "STAKING_CONTRACT_ID" \
  --amount 1000
```

#### Get Contract Information

```bash
stellar-defi-cli get-info \
  --contract-id "CONTRACT_ID"
```

### Library Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
stellar-defi-toolkit = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
```

#### Example: Deploy a Token Contract

```rust
use stellar_defi_toolkit::{TokenContract, StellarClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = StellarClient::new().await?;
    
    let token = TokenContract::new("My Token".to_string(), "MTK".to_string(), 1000000);
    let contract_id = token.deploy(&client).await?;
    
    println!("Token deployed with contract ID: {}", contract_id);
    Ok(())
}
```

#### Example: Create a Liquidity Pool

```rust
use stellar_defi_toolkit::{LiquidityPoolContract, StellarClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = StellarClient::new().await?;
    
    let pool = LiquidityPoolContract::new(
        "TOKEN_A_CONTRACT_ID".to_string(),
        "TOKEN_B_CONTRACT_ID".to_string()
    );
    let contract_id = pool.deploy(&client).await?;
    
    println!("Liquidity pool created with contract ID: {}", contract_id);
    Ok(())
}
```

#### Example: Stake Tokens and Earn Rewards

```rust
use soroban_sdk::{Env, Address};
use stellar_defi_toolkit::StakingContract;

fn main() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    // Initialize staking contract
    let staking = StakingContractClient::new(&env, &contract_id);
    staking.initialize(
        &admin,
        &staking_token_address,
        &reward_token_address,
        &17280, // 1 day reward period
    );
    
    // Set rewards
    staking.notify_reward_amount(&admin, &10_000_000_000);
    
    // User stakes tokens
    staking.stake(&user, &1_000_000_000);
    
    // Check earned rewards
    let earned = staking.get_earned(&user);
    println!("Earned rewards: {}", earned);
    
    // Claim rewards
    staking.claim_rewards(&user);
}
```

For more details, see [STAKING_README.md](STAKING_README.md) and [docs/staking_contract.md](docs/staking_contract.md).

## 🏗️ Project Structure

```
stellar-defi-toolkit/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library entry point
│   ├── contracts/           # Smart contract implementations
│   │   ├── mod.rs
│   │   ├── token.rs         # Token contract
│   │   ├── liquidity_pool.rs # Liquidity pool contract
│   │   ├── staking.rs       # Staking contract
│   │   ├── governance.rs    # Governance contract
│   │   ├── circuit_breaker.rs # Circuit breaker for volatility protection
│   │   ├── price_oracle.rs  # Multi-source price oracle
│   │   └── oracle_manager.rs # Oracle aggregation manager
│   ├── utils/               # Utility functions
│   │   ├── mod.rs
│   │   ├── client.rs        # Stellar client
│   │   └── helpers.rs       # Helper functions
│   └── types/               # Type definitions
│       ├── mod.rs
│       ├── token.rs
│       └── pool.rs
├── tests/                   # Integration tests
├── examples/               # Example usage
│   ├── circuit_breaker_demo.rs # Circuit breaker demonstration
│   └── ...
├── docs/                   # Documentation
│   ├── circuit_breaker_guide.md # Circuit breaker guide
│   └── ...
├── Cargo.toml
└── README.md
```

## 🛡️ Circuit Breaker System

The toolkit includes a comprehensive circuit breaker system to protect against extreme price volatility:

### Key Features

- **Automatic Protection**: Halts operations when price changes exceed 10% in a single update
- **Consecutive Deviation Detection**: Trips after 3 consecutive 5%+ price movements
- **Rate Limiting**: Enforces 5-minute minimum intervals between price updates
- **Per-Asset Control**: Independent circuit breaker status for each asset
- **Admin Controls**: Manual trip, reset, and configuration management

### How It Works

```rust
// Circuit breaker automatically checks price volatility
if !price_oracle.is_operational(env.clone(), asset_address.clone()) {
    panic!("Circuit breaker tripped - operations halted");
}

// Get price (includes automatic circuit breaker check)
let price = price_oracle.get_price(env, asset_address);
```

### Protection Thresholds

| Threshold | Value | Action |
|-----------|-------|--------|
| Single Deviation | 10% | Immediate trip |
| Consecutive Deviation | 5% | Count towards trip |
| Consecutive Count | 3 updates | Trip circuit breaker |
| Rate Limit | 5 minutes | Minimum update interval |
| Cooldown | 30 minutes | Time before recovery |

### Benefits

✓ Prevents liquidations during flash crashes  
✓ Protects against oracle manipulation  
✓ Provides time for admin review during extreme volatility  
✓ Reduces systemic risk from cascading failures  
✓ Enables safe recovery after market disruptions  

See the [Circuit Breaker Guide](docs/circuit_breaker_guide.md) for detailed documentation.

## 🔧 Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Running Examples

```bash
cargo run --example token_deployment
cargo run --example liquidity_pool
```

## 📚 Documentation

- [Circuit Breaker Guide](docs/circuit_breaker_guide.md) - Price volatility protection
- [Risk Management Framework](docs/synthetic_protocol_risk_management.md) - Comprehensive risk controls
- [Soroban Documentation](https://soroban.stellar.org/)
- [Stellar Documentation](https://developers.stellar.org/)
- [API Reference](https://docs.rs/stellar-defi-toolkit/)

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under either of:

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  https://opensource.org/licenses/MIT)

at your option.

## 🙏 Acknowledgments

- The [Stellar Development Foundation](https://stellar.org/) for the amazing Soroban platform
- The Rust community for excellent tooling and ecosystem
- All contributors who help make this project better

## �️ Roadmap

### Phase 1: Core DeFi Components (Q1 2024)
- [x] Token contracts with ERC-20-like functionality
- [x] Liquidity pools with AMM functionality
- [x] Staking contracts with time-based reward distribution
- [x] Basic CLI tools for contract deployment
- [x] Comprehensive testing suite

### Phase 2: Advanced Features (Q2 2024)
- [ ] Yield farming protocols
- [ ] Cross-chain bridges
- [ ] Governance contracts with voting
- [ ] Advanced analytics dashboard
- [ ] Web GUI for easy interaction

### Phase 3: Ecosystem Integration (Q3 2024)
- [ ] Integration with major DEXs
- [ ] Oracle integration for price feeds
- [ ] Multi-token governance
- [ ] Automated strategy execution
- [ ] Mobile app support

### Phase 4: Enterprise Features (Q4 2024)
- [ ] Institutional-grade security
- [ ] Compliance tools
- [ ] Advanced risk management
- [ ] White-label solutions
- [ ] Enterprise support packages

### Future Enhancements
- [ ] Layer 2 scaling solutions
- [ ] AI-powered trading strategies
- [ ] Social trading features
- [ ] NFT integration
- [ ] DeFi insurance protocols

## �📞 Support

- 📧 Email: support@stellar-defi-toolkit.com
- 💬 Discord: [Join our community](https://discord.gg/stellar-defi-toolkit)
- 🐦 Twitter: [@stellardefi](https://twitter.com/stellardefi)

---

**Built with ❤️ for the Stellar ecosystem**
