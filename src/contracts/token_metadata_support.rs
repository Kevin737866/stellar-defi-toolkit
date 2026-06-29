//! Token metadata support using Soroban standard token interface (issue #21)

use soroban_sdk::{contract, contractimpl, contracttype, Env, String};

#[contracttype]
pub enum MetaKey {
    Name,
    Symbol,
    Decimals,
}

#[contract]
pub struct TokenMetadataContract;

#[contractimpl]
impl TokenMetadataContract {
    pub fn initialize(env: Env, name: String, symbol: String, decimals: u32) {
        env.storage().instance().set(&MetaKey::Name, &name);
        env.storage().instance().set(&MetaKey::Symbol, &symbol);
        env.storage().instance().set(&MetaKey::Decimals, &decimals);
    }

    pub fn name(env: Env) -> String {
        env.storage().instance().get(&MetaKey::Name).unwrap()
    }

    pub fn symbol(env: Env) -> String {
        env.storage().instance().get(&MetaKey::Symbol).unwrap()
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&MetaKey::Decimals).unwrap_or(7)
    }
}
