//! Soroban SDK token contract implementation (issue #20)

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

#[contracttype]
pub enum TokenKey {
    Balance(Address),
    TotalSupply,
    Admin,
}

#[contract]
pub struct SorobanTokenContract;

#[contractimpl]
impl SorobanTokenContract {
    pub fn initialize(env: Env, admin: Address, supply: i128) {
        admin.require_auth();
        env.storage().instance().set(&TokenKey::Admin, &admin);
        env.storage().instance().set(&TokenKey::TotalSupply, &supply);
        env.storage().instance().set(&TokenKey::Balance(admin.clone()), &supply);
    }

    pub fn balance(env: Env, owner: Address) -> i128 {
        env.storage().instance().get(&TokenKey::Balance(owner)).unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        assert!(amount > 0, "amount must be positive");

        let from_balance: i128 = env.storage().instance().get(&TokenKey::Balance(from.clone())).unwrap_or(0);
        assert!(from_balance >= amount, "insufficient balance");

        let to_balance: i128 = env.storage().instance().get(&TokenKey::Balance(to.clone())).unwrap_or(0);

        env.storage().instance().set(&TokenKey::Balance(from), &(from_balance - amount));
        env.storage().instance().set(&TokenKey::Balance(to), &(to_balance + amount));
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage().instance().get(&TokenKey::TotalSupply).unwrap_or(0)
    }

    pub fn mint(env: Env, admin: Address, to: Address, amount: i128) {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&TokenKey::Admin).unwrap();
        assert!(admin == stored_admin, "unauthorized");

        let to_balance: i128 = env.storage().instance().get(&TokenKey::Balance(to.clone())).unwrap_or(0);
        let supply: i128 = env.storage().instance().get(&TokenKey::TotalSupply).unwrap_or(0);

        env.storage().instance().set(&TokenKey::Balance(to), &(to_balance + amount));
        env.storage().instance().set(&TokenKey::TotalSupply, &(supply + amount));
    }
}
