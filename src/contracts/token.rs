use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, symbol_short};

const ADMIN_KEY: Symbol = symbol_short!("ADMIN");

fn balance_key(env: &Env, owner: &Address) -> soroban_sdk::Bytes {
    let mut key = soroban_sdk::Bytes::new(env);
    key.append(&soroban_sdk::Bytes::from_slice(env, b"bal:"));
    key.append(&owner.clone().to_string().into_bytes(env));
    key
}

fn allowance_key(env: &Env, owner: &Address, spender: &Address) -> soroban_sdk::Bytes {
    let mut key = soroban_sdk::Bytes::new(env);
    key.append(&soroban_sdk::Bytes::from_slice(env, b"allow:"));
    key.append(&owner.clone().to_string().into_bytes(env));
    key.append(&soroban_sdk::Bytes::from_slice(env, b":"));
    key.append(&spender.clone().to_string().into_bytes(env));
    key
}

#[contract]
pub struct TokenContract;

#[contractimpl]
impl TokenContract {
    /// Initialize the contract and set the admin
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&ADMIN_KEY, &admin);
    }

    /// Returns the balance of the given address using persistent storage
    pub fn balance_of(env: Env, owner: Address) -> i128 {
        let key = balance_key(&env, &owner);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Returns the allowance that `spender` is allowed to spend on behalf of `owner`
    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        let key = allowance_key(&env, &owner, &spender);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Approve `spender` to spend up to `amount` tokens on behalf of the caller
    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        owner.require_auth();
        let key = allowance_key(&env, &owner, &spender);
        env.storage().persistent().set(&key, &amount);
    }

    /// Transfer tokens from the caller to `to`
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_balance = Self::balance_of(env.clone(), from.clone());
        assert!(from_balance >= amount, "Insufficient balance");

        let from_key = balance_key(&env, &from);
        let to_key = balance_key(&env, &to);

        let to_balance = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&from_key, &(from_balance - amount));
        env.storage().persistent().set(&to_key, &(to_balance + amount));
    }

    /// Transfer tokens on behalf of `from` using an allowance
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let current_allowance = Self::allowance(env.clone(), from.clone(), spender.clone());
        assert!(current_allowance >= amount, "Insufficient allowance");

        // Reduce allowance
        let allow_key = allowance_key(&env, &from, &spender);
        env.storage().persistent().set(&allow_key, &(current_allowance - amount));

        // Execute transfer
        let from_balance = Self::balance_of(env.clone(), from.clone());
        assert!(from_balance >= amount, "Insufficient balance");

        let from_key = balance_key(&env, &from);
        let to_key = balance_key(&env, &to);
        let to_balance = Self::balance_of(env.clone(), to.clone());

        env.storage().persistent().set(&from_key, &(from_balance - amount));
        env.storage().persistent().set(&to_key, &(to_balance + amount));
    }

    /// Mint new tokens to `to` — admin only
    pub fn mint(env: Env, to: Address, amount: i128) {
        // Enforce admin-only access
        let admin: Address = env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized");
        admin.require_auth();

        let key = balance_key(&env, &to);
        let current = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&key, &(current + amount));
    }

    /// Burn tokens from `from` — caller must be the token holder or admin
    pub fn burn(env: Env, from: Address, amount: i128) {
        // Enforce that either the holder or the admin is authorizing the burn
        let admin: Address = env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized");

        // Require auth from either the holder (self-burn) or the admin
        if from != admin {
            from.require_auth();
        } else {
            admin.require_auth();
        }

        let current = Self::balance_of(env.clone(), from.clone());
        assert!(current >= amount, "Insufficient balance to burn");

        let key = balance_key(&env, &from);
        env.storage().persistent().set(&key, &(current - amount));
    }

    /// Get the current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized")
    }
}
