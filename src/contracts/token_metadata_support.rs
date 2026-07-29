//! Token metadata support — full SEP-41 token standard (issue #220)
//!
//! Provides `name`, `symbol`, `decimals`, and `token_uri` on-chain, together
//! with metadata-update events on initialization.

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Env, String};

// ─── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum MetaKey {
    Name,
    Symbol,
    Decimals,
    /// SEP-41 extension: off-chain metadata URI (e.g. IPFS, HTTPS).
    TokenUri,
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct TokenMetadataContract;

#[contractimpl]
impl TokenMetadataContract {
    /// Initialise metadata and emit a `MetadataSet` event.
    ///
    /// # Arguments
    /// * `name`      – Human-readable token name.
    /// * `symbol`    – Ticker symbol (e.g. `"XLM"`).
    /// * `decimals`  – Number of decimal places (Stellar standard: 7).
    /// * `token_uri` – Off-chain metadata URI (empty string = not set).
    pub fn initialize(
        env: Env,
        name: String,
        symbol: String,
        decimals: u32,
        token_uri: String,
    ) {
        env.storage().instance().set(&MetaKey::Name, &name);
        env.storage().instance().set(&MetaKey::Symbol, &symbol);
        env.storage().instance().set(&MetaKey::Decimals, &decimals);
        env.storage().instance().set(&MetaKey::TokenUri, &token_uri);

        // Emit a metadata-set event so indexers can track it.
        env.events().publish(
            (symbol_short!("meta_set"), symbol_short!("init")),
            (name, symbol, decimals),
        );
    }

    // ─── SEP-41 Required ──────────────────────────────────────────────────────

    /// Return the token name (SEP-41 required).
    pub fn name(env: Env) -> String {
        env.storage()
            .instance()
            .get(&MetaKey::Name)
            .unwrap_or_else(|| String::from_str(&env, ""))
    }

    /// Return the token symbol (SEP-41 required).
    pub fn symbol(env: Env) -> String {
        env.storage()
            .instance()
            .get(&MetaKey::Symbol)
            .unwrap_or_else(|| String::from_str(&env, ""))
    }

    /// Return the number of decimals (SEP-41 required, Stellar default: 7).
    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&MetaKey::Decimals)
            .unwrap_or(7)
    }

    // ─── SEP-41 Extension ─────────────────────────────────────────────────────

    /// Return the token metadata URI (SEP-41 extension).
    ///
    /// Returns an empty `String` when no URI has been set.
    pub fn token_uri(env: Env) -> String {
        env.storage()
            .instance()
            .get(&MetaKey::TokenUri)
            .unwrap_or_else(|| String::from_str(&env, ""))
    }

    /// Update the metadata URI and emit a `MetadataUpdated` event.
    pub fn set_token_uri(env: Env, token_uri: String) {
        env.storage().instance().set(&MetaKey::TokenUri, &token_uri);
        env.events().publish(
            (symbol_short!("meta_upd"), symbol_short!("uri")),
            token_uri,
        );
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Events, Env, IntoVal};

    fn setup() -> (Env, soroban_sdk::Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, TokenMetadataContract);
        (env, contract_id)
    }

    #[test]
    fn test_name_returns_token_name() {
        let (env, contract_id) = setup();
        let client = TokenMetadataContractClient::new(&env, &contract_id);
        client.initialize(
            &String::from_str(&env, "Stellar Token"),
            &String::from_str(&env, "XLM"),
            &7,
            &String::from_str(&env, ""),
        );
        assert_eq!(
            client.name(),
            String::from_str(&env, "Stellar Token")
        );
    }

    #[test]
    fn test_symbol_returns_token_symbol() {
        let (env, contract_id) = setup();
        let client = TokenMetadataContractClient::new(&env, &contract_id);
        client.initialize(
            &String::from_str(&env, "Stellar Token"),
            &String::from_str(&env, "XLM"),
            &7,
            &String::from_str(&env, ""),
        );
        assert_eq!(
            client.symbol(),
            String::from_str(&env, "XLM")
        );
    }

    #[test]
    fn test_decimals_returns_configured_value() {
        let (env, contract_id) = setup();
        let client = TokenMetadataContractClient::new(&env, &contract_id);
        client.initialize(
            &String::from_str(&env, "T"),
            &String::from_str(&env, "T"),
            &7,
            &String::from_str(&env, ""),
        );
        assert_eq!(client.decimals(), 7);
    }

    #[test]
    fn test_token_uri_returns_set_value() {
        let (env, contract_id) = setup();
        let client = TokenMetadataContractClient::new(&env, &contract_id);
        let uri = "https://example.com/token.json";
        client.initialize(
            &String::from_str(&env, "Token"),
            &String::from_str(&env, "TKN"),
            &7,
            &String::from_str(&env, uri),
        );
        assert_eq!(
            client.token_uri(),
            String::from_str(&env, uri)
        );
    }

    #[test]
    fn test_set_token_uri_updates_value() {
        let (env, contract_id) = setup();
        let client = TokenMetadataContractClient::new(&env, &contract_id);
        client.initialize(
            &String::from_str(&env, "Token"),
            &String::from_str(&env, "TKN"),
            &7,
            &String::from_str(&env, ""),
        );

        let new_uri = "ipfs://Qm123/metadata.json";
        client.set_token_uri(&String::from_str(&env, new_uri));

        assert_eq!(
            client.token_uri(),
            String::from_str(&env, new_uri)
        );
    }

    #[test]
    fn test_metadata_event_emitted_on_initialize() {
        let (env, contract_id) = setup();
        let client = TokenMetadataContractClient::new(&env, &contract_id);
        client.initialize(
            &String::from_str(&env, "DeFi Token"),
            &String::from_str(&env, "DFT"),
            &7,
            &String::from_str(&env, ""),
        );

        // At least one event should have been published during initialize.
        let events = env.events().all();
        assert!(
            !events.is_empty(),
            "Expected metadata event to be emitted on initialization"
        );
    }
}
