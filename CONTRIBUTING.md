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
