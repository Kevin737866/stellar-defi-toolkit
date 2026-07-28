# Contributing to Stellar DeFi Toolkit

Thank you for your interest in contributing to Stellar DeFi Toolkit! This document provides guidelines and information for contributors.

## 🚀 Getting Started

### Prerequisites

- Rust 1.70.0 or higher
- Stellar CLI tools
- Soroban CLI
- Git

### Development Setup

1. **Fork the Repository**
   ```bash
   # Fork the repository on GitHub and clone your fork
   git clone https://github.com/YOUR_USERNAME/stellar-defi-toolkit.git
   cd stellar-defi-toolkit
   ```

2. **Set Up Development Environment**
   ```bash
   # Install Rust dependencies
   cargo build
   
   # Run tests to ensure everything works
   cargo test
   
   # Install development tools
   cargo install cargo-watch cargo-expand
   ```

3. **Create a Development Branch**
   ```bash
   git checkout -b feature/your-feature-name
   ```

## 📋 Development Guidelines

### Code Style

We use the standard Rust formatting and linting tools:

```bash
# Format code
cargo fmt

# Run clippy for linting
cargo clippy -- -D warnings

# Run both together
cargo fmt && cargo clippy -- -D warnings
```

### Testing

All contributions must include tests:

```bash
# Run all tests
cargo test

# Run tests with coverage
cargo tarpaulin --out Html

# Run integration tests
cargo test --test integration_tests

# Run the gas/compute-cost benchmark suite (see docs/gas_benchmarks.md)
cargo bench --bench lending_benchmarks
```

### Continuous Integration

Every push and pull request against `main` runs `.github/workflows/ci.yml`,
which gates merges on:

- **`test`** — `cargo test --all-targets` across a matrix of Rust versions
  (MSRV, stable, beta).
- **`clippy`** — `cargo clippy --all-targets --all-features -- -D warnings`;
  any lint fails the build.
- **`coverage`** — `cargo tarpaulin` with a minimum **80%** line-coverage
  gate; the HTML/XML report is uploaded as a workflow artifact.
- **`security-audit`** — `cargo audit` against the advisory database.
- **`gas-benchmarks`** — runs `benches/lending_benchmarks.rs` in `--test`
  mode as a correctness smoke test on every PR (it does not itself fail on a
  performance regression — see `docs/gas_benchmarks.md` for how to compare
  a branch against a baseline before merging).

Run the same checks locally before opening a PR:

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo tarpaulin --fail-under 80
cargo audit
```

### Documentation

- Add documentation comments (`///`) for all public functions and types
- Update README.md if adding new features
- Add examples to the `examples/` directory for new functionality

## 🏗️ Project Structure

```
stellar-defi-toolkit/
├── src/
│   ├── contracts/           # Smart contract implementations
│   ├── utils/              # Utility functions
│   ├── types/              # Type definitions
│   ├── main.rs            # CLI entry point
│   └── lib.rs             # Library entry point
├── tests/                 # Integration tests
├── examples/              # Usage examples
├── docs/                 # Documentation
└── README.md
```

## 🤝 Contribution Types

### Bug Reports

When reporting bugs, please include:

1. **Environment**: OS, Rust version, Stellar network
2. **Steps to Reproduce**: Clear, numbered steps
3. **Expected Behavior**: What should happen
4. **Actual Behavior**: What actually happened
5. **Error Messages**: Complete error output

### Feature Requests

1. **Use Case**: Describe the problem you're trying to solve
2. **Proposed Solution**: How you envision the feature working
3. **Alternatives**: Other approaches you've considered
4. **Additional Context**: Any relevant information

### Code Contributions

1. **Small, Focused Changes**: Keep PRs focused on a single feature or fix
2. **Test Coverage**: Ensure new code is well-tested
3. **Documentation**: Update relevant documentation
4. **Commit Messages**: Use clear, descriptive commit messages

#### Commit Message Format

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or modifying tests
- `chore`: Maintenance tasks

Examples:
```
feat(token): add burn functionality
fix(pool): correct price calculation in swaps
docs(readme): update installation instructions
```

## 🧪 Testing Guidelines

### Unit Tests

- Test individual functions and methods
- Cover edge cases and error conditions
- Use descriptive test names

```rust
#[test]
fn test_token_minting_with_valid_amount() {
    // Arrange
    let mut token = TokenContract::new("Test", "TEST", 1000);
    let recipient = Address::generate(&Env::default());
    
    // Act
    let result = token.mint(recipient, 500);
    
    // Assert
    assert!(result.is_ok());
    assert_eq!(token.total_supply, 1500);
}
```

### Integration Tests

- Test contract interactions
- Use testnet when possible
- Mock external dependencies

## 📝 Documentation Standards

### Code Documentation

```rust
/// Deploys a new token contract to the Stellar network.
/// 
/// # Arguments
/// 
/// * `client` - The Stellar client for network interactions
/// 
/// # Returns
/// 
/// Returns the contract ID of the deployed token
/// 
/// # Examples
/// 
/// ```rust
/// let client = StellarClient::new().await?;
/// let token = TokenContract::new("My Token", "MTK", 1000000);
/// let contract_id = token.deploy(&client).await?;
/// ```
pub async fn deploy(&self, client: &StellarClient) -> Result<String> {
    // Implementation
}
```

### README Documentation

- Keep installation instructions up to date
- Include usage examples
- Add new features to the features list

## 🔐 Access Control & Permission Documentation

Every contract in `src/contracts/` is documented in
[`docs/ACCESS_CONTROL_MATRIX.md`](docs/ACCESS_CONTROL_MATRIX.md), which maps
**Contract × Role × Action** for the whole protocol. The four roles are:

- **Admin** — a single privileged address managing parameters, pausing, and
  emergency actions for a contract.
- **Governance** — token-weighted voting participants (proposers, voters, executors).
- **Keeper** — a permissionless or semi-permissionless automated caller (bot, oracle
  feeder, liquidator, arbitrageur).
- **User** — any account interacting with the protocol on its own behalf.

If your change adds, removes, or changes who can call a function:

1. **Update `docs/ACCESS_CONTROL_MATRIX.md`** — add/edit the row for that function in
   the relevant contract's table, and update the role capability rollup sections if
   the change is significant.
2. **Add or update the module-level `## Access Control` doc comment** at the top of
   the contract file (see any file in `src/contracts/` for the established format —
   e.g. `staking.rs` or `stability_pool.rs`). This keeps a short, accurate summary next
   to the code, while the matrix stays the canonical, cross-contract reference.
3. **State enforcement, not just intent.** Document what the code actually does
   (e.g. "no `require_auth()` call" or "gated by a broken admin check"), not just what
   the function name or a stale comment implies. The matrix is only useful if it
   reflects reality — inaccurate access-control docs are worse than none.
4. If your change *fixes* a broken or missing auth check, update the corresponding
   row(s) and the [Enforcement Gap Appendix](docs/ACCESS_CONTROL_MATRIX.md#appendix-enforcement-gaps-found-during-this-audit)
   to mark the finding resolved.

## 🛡️ Security

Before opening a PR that touches fund-handling logic (minting, burning, transfers,
collateral, liquidations, oracle prices, or governance execution), review
[`docs/SECURITY_AUDIT_CHECKLIST.md`](docs/SECURITY_AUDIT_CHECKLIST.md) and the threat
model it contains. At minimum:

- Walk through the reentrancy, overflow, oracle-manipulation, governance-attack, and
  flash-loan-attack checklist items relevant to your change.
- Call out any new external calls, price dependencies, or privileged operations in
  your PR description.
- If your change touches an access-control check, cross-reference
  `docs/ACCESS_CONTROL_MATRIX.md` (see previous section).

To report a security vulnerability, follow the process in
[`.github/SECURITY.md`](.github/SECURITY.md) rather than opening a public issue.

## 🔄 Pull Request Process

1. **Create Pull Request**
   - Use descriptive title and description
   - Link to relevant issues
   - Include screenshots if applicable

2. **Code Review**
   - Address reviewer feedback promptly
   - Keep discussions constructive
   - Update PR as needed

3. **Merge Requirements**
   - All tests pass
   - Code coverage maintained or improved
   - Documentation updated
   - At least one approval from maintainers

## 🏛️ Governance

Changes to deployed protocol parameters or code follow a formal governance
process, separate from the code-review process above. The full process —
change classification, proposal requirements, voting, timelock, execution, and
post-upgrade verification — is documented in
[docs/upgrade_governance_process.md](docs/upgrade_governance_process.md).
Component reference (voting contracts, timelock crate, multisig paths) is in
[governance/README.md](governance/README.md).

### When this applies to your PR

| Your change | Governance track | Where |
|-------------|-------------------|-------|
| Bug fix, refactor, test, docs, tooling | None — normal PR review (above) | This file |
| A constant that changes user economics (collateral ratio, fee, liquidation penalty/bonus, interest rate model, circuit-breaker threshold, cap) | **C3 — Economic**, governance vote required before deployment | [Process §2](docs/upgrade_governance_process.md#2-change-classification) |
| New contract WASM, changed storage layout, new module | **C4 — Code upgrade**, governance vote + audit required | [Process §2](docs/upgrade_governance_process.md#2-change-classification) |
| Voting parameters, admin/multisig membership, treasury control | **C5 — Constitutional**, highest bar | [Process §2](docs/upgrade_governance_process.md#2-change-classification) |

A PR that changes a C3+ constant is welcome as a **proposal**, not as a silent
merge. Open it using the
[upgrade proposal template](docs/templates/upgrade_proposal_template.md) and
link the PR from the proposal issue — merging the code does not deploy it or
change any live parameter; that only happens through the on-chain governance
flow once the proposal passes.

### Writing a governance proposal

1. Classify the change (C1–C5) per [process §2](docs/upgrade_governance_process.md#2-change-classification).
2. Copy [docs/templates/upgrade_proposal_template.md](docs/templates/upgrade_proposal_template.md)
   into a new issue labelled `governance`.
3. For any change to collateral ratios, liquidation penalties/bonuses, or
   interest rate models, complete the **Economic impact** section using the
   framework in [docs/economic_risk_analysis.md](docs/economic_risk_analysis.md) —
   in particular, show that the liquidation toxic-zone identity
   (`c' = (c − f(1+b))/(1−f)`) still improves positions across the permitted
   liquidation band. Proposals that skip this are sent back in review.
4. Reviewers evaluate against the eight criteria in
   [process §4.1](docs/upgrade_governance_process.md#41-review-criteria) —
   correctness, economic soundness, parameter coherence, storage compatibility,
   authorisation, blast radius, observability, reversibility.
5. Once approved, the proposal moves to on-chain voting, then a timelock, then
   execution, then the [post-upgrade verification checklist](docs/upgrade_governance_process.md#7-stage-5--post-upgrade-verification).
   A change is not "done" until Stage 5 is complete.

### Current governance status

Cross-contract execution from a passed governance proposal is not yet fully
wired (`execute_proposal_logic` emits events without calling target contracts —
see [governance/README.md — Known gaps](governance/README.md#known-gaps)). Until
that lands, ratified proposals are applied by the admin multisig, and every such
application must cite the proposal ID. If your contribution touches
`governance_v2.rs`, `synthetic_governance.rs`, or the `governance/` crate, closing
one of the items in that gap list is high-value, reviewer-prioritised work.

## 🚨 Emergency Procedures

If you discover a security vulnerability, an active exploit, or believe the
protocol is at risk **right now**, do not open a normal GitHub issue or PR first.

1. Follow the [Emergency Response Runbook](docs/emergency_response_runbook.md) —
   it has a 5-minute first-response checklist and module-specific procedures for
   exploits, oracle failures, market crashes, and governance attacks.
2. If funds are at risk, contact the security intake channel in the runbook's
   [contact list](docs/emergency_response_runbook.md#10-contact-list-and-escalation-path)
   before writing anything public. Public disclosure follows a 24-hour timeline
   documented in the runbook, not an immediate post.
3. For a confirmed, non-active vulnerability (found in review, not being
   exploited), use the [Security Incident Report template](.github/ISSUE_TEMPLATE/incident_report.yml)
   or the private intake — whichever the runbook's contact list designates for
   the severity.
4. Emergency pause/shutdown actions taken outside normal PR review are still
   governed: they require a ratifying governance proposal within 72 hours per
   [upgrade_governance_process.md §9](docs/upgrade_governance_process.md#9-emergency-changes-c1).
   If your work involves exercising or reviewing an emergency action, that
   ratification proposal is part of the deliverable, not an afterthought.

Do not include exploit details, working proofs-of-concept, or specific attack
transactions in a public PR description or commit message. Reference the
private incident by ID instead, and let the disclosure timeline in the runbook
govern when details become public.

## 🏆 Recognition

Contributors are recognized in several ways:

- **Contributors.md**: List of all contributors
- **Release Notes**: Mentioned in changelog
- **Community**: Highlighted in discussions and announcements

## 📞 Getting Help

- **Discord**: [Join our community](https://discord.gg/stellar-defi-toolkit)
- **GitHub Issues**: Open an issue for questions or problems
- **Documentation**: Check existing docs and examples

## 📜 Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please:

- Be respectful and considerate
- Use inclusive language
- Focus on constructive feedback
- Help others learn and grow

## 🚀 Release Process

1. **Version Bumping**: Update version in Cargo.toml
2. **Changelog**: Update CHANGELOG.md
3. **Tagging**: Create git tag with version number
4. **Publishing**: Publish to crates.io
5. **Announcement**: Post release notes and announcements

## 📚 Resources

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [API Reference](https://docs.rs/stellar-defi-toolkit/)

---

Thank you for contributing to Stellar DeFi Toolkit! 🚀
