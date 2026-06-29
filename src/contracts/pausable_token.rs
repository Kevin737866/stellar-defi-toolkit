//! Pausable transfer mechanism for emergency token lockdown (issue #22)

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
pub enum PausableKey {
    Paused,
    Admin,
    Balance(Address),
}

#[contract]
pub struct PausableTokenContract;

#[contractimpl]
impl PausableTokenContract {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&PausableKey::Admin, &admin);
        env.storage().instance().set(&PausableKey::Paused, &false);
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");
        env.storage().instance().set(&PausableKey::Paused, &true);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");
        env.storage().instance().set(&PausableKey::Paused, &false);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&PausableKey::Paused).unwrap_or(false)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let paused: bool = env.storage().instance().get(&PausableKey::Paused).unwrap_or(false);
        assert!(!paused, "transfers are paused");
        assert!(amount > 0, "amount must be positive");

        let from_balance: i128 = env.storage().instance().get(&PausableKey::Balance(from.clone())).unwrap_or(0);
        assert!(from_balance >= amount, "insufficient balance");

        let to_balance: i128 = env.storage().instance().get(&PausableKey::Balance(to.clone())).unwrap_or(0);
        env.storage().instance().set(&PausableKey::Balance(from), &(from_balance - amount));
        env.storage().instance().set(&PausableKey::Balance(to), &(to_balance + amount));
    }
}
