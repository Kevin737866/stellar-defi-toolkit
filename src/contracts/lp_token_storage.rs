//! LP token storage tracking user balances in the liquidity pool (issue #23)

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
pub enum LpStorageKey {
    Balance(Address),
    TotalSupply,
}

#[contract]
pub struct LpTokenStorage;

#[contractimpl]
impl LpTokenStorage {
    pub fn mint(env: Env, to: Address, amount: i128) {
        assert!(amount > 0, "amount must be positive");

        let current: i128 = env.storage().instance().get(&LpStorageKey::Balance(to.clone())).unwrap_or(0);
        let supply: i128 = env.storage().instance().get(&LpStorageKey::TotalSupply).unwrap_or(0);

        env.storage().instance().set(&LpStorageKey::Balance(to), &(current + amount));
        env.storage().instance().set(&LpStorageKey::TotalSupply, &(supply + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        assert!(amount > 0, "amount must be positive");

        let current: i128 = env.storage().instance().get(&LpStorageKey::Balance(from.clone())).unwrap_or(0);
        assert!(current >= amount, "insufficient lp balance");

        let supply: i128 = env.storage().instance().get(&LpStorageKey::TotalSupply).unwrap_or(0);

        env.storage().instance().set(&LpStorageKey::Balance(from), &(current - amount));
        env.storage().instance().set(&LpStorageKey::TotalSupply, &(supply - amount));
    }

    pub fn balance(env: Env, owner: Address) -> i128 {
        env.storage().instance().get(&LpStorageKey::Balance(owner)).unwrap_or(0)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage().instance().get(&LpStorageKey::TotalSupply).unwrap_or(0)
    }
}
