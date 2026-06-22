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
    /// Initialize the contract and set the admin.
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&ADMIN_KEY, &admin);
    }

    /// Returns the balance of `owner`.
    pub fn balance_of(env: Env, owner: Address) -> i128 {
        let key = balance_key(&env, &owner);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Returns the allowance that `spender` may spend on behalf of `owner`.
    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        let key = allowance_key(&env, &owner, &spender);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Approve `spender` to spend up to `amount` tokens on behalf of `owner`.
    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        owner.require_auth();
        let key = allowance_key(&env, &owner, &spender);
        env.storage().persistent().set(&key, &amount);
        env.events().publish(
            (symbol_short!("approval"), owner, spender),
            amount,
        );
    }

    /// Transfer `amount` tokens from `from` to `to`. Caller must be `from`.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_balance = Self::balance_of(env.clone(), from.clone());
        assert!(from_balance >= amount, "Insufficient balance");

        let to_balance = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&balance_key(&env, &from), &(from_balance - amount));
        env.storage().persistent().set(&balance_key(&env, &to), &(to_balance + amount));

        env.events().publish(
            (symbol_short!("transfer"), from, to),
            amount,
        );
    }

    /// Transfer `amount` tokens from `from` to `to` using `spender`'s allowance.
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();

        // 1. Check and deduct allowance
        let current_allowance = Self::allowance(env.clone(), from.clone(), spender.clone());
        assert!(current_allowance >= amount, "Insufficient allowance");
        env.storage().persistent().set(
            &allowance_key(&env, &from, &spender),
            &(current_allowance - amount),
        );

        // 2. Check and update balances
        let from_balance = Self::balance_of(env.clone(), from.clone());
        assert!(from_balance >= amount, "Insufficient balance");
        let to_balance = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&balance_key(&env, &from), &(from_balance - amount));
        env.storage().persistent().set(&balance_key(&env, &to), &(to_balance + amount));

        // 3. Emit Transfer event
        env.events().publish(
            (symbol_short!("transfer"), from, to),
            amount,
        );
    }

    /// Mint `amount` tokens to `to`. Admin only.
    pub fn mint(env: Env, to: Address, amount: i128) {
        let admin: Address = env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized");
        admin.require_auth();

        let key = balance_key(&env, &to);
        let current = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&key, &(current + amount));
    }

    /// Burn `amount` tokens from `from`. Caller must be `from` or the admin.
    pub fn burn(env: Env, from: Address, amount: i128) {
        let admin: Address = env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized");
        if from != admin {
            from.require_auth();
        } else {
            admin.require_auth();
        }

        let current = Self::balance_of(env.clone(), from.clone());
        assert!(current >= amount, "Insufficient balance to burn");
        env.storage().persistent().set(&balance_key(&env, &from), &(current - amount));
    }

    /// Returns the current admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, soroban_sdk::Address, TokenContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);
        // Leak env for 'static lifetime required by Client — acceptable in tests
        let env: &'static Env = Box::leak(Box::new(env));
        let client = TokenContractClient::new(env, &contract_id);
        (env.clone(), admin, client)
    }

    #[test]
    fn test_mint_and_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let user = soroban_sdk::Address::generate(&env);
        client.mint(&user, &1_000);
        assert_eq!(client.balance_of(&user), 1_000);
    }

    #[test]
    fn test_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let sender = soroban_sdk::Address::generate(&env);
        let receiver = soroban_sdk::Address::generate(&env);
        client.mint(&sender, &1_000);

        client.transfer(&sender, &receiver, &400);
        assert_eq!(client.balance_of(&sender), 600);
        assert_eq!(client.balance_of(&receiver), 400);
    }

    #[test]
    #[should_panic(expected = "Insufficient balance")]
    fn test_transfer_insufficient_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let sender = soroban_sdk::Address::generate(&env);
        let receiver = soroban_sdk::Address::generate(&env);
        client.mint(&sender, &100);
        client.transfer(&sender, &receiver, &500);
    }

    #[test]
    fn test_approve_and_allowance() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let owner = soroban_sdk::Address::generate(&env);
        let spender = soroban_sdk::Address::generate(&env);
        client.approve(&owner, &spender, &300);
        assert_eq!(client.allowance(&owner, &spender), 300);
    }

    #[test]
    fn test_transfer_from_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let owner = soroban_sdk::Address::generate(&env);
        let spender = soroban_sdk::Address::generate(&env);
        let receiver = soroban_sdk::Address::generate(&env);

        client.mint(&owner, &1_000);
        client.approve(&owner, &spender, &500);

        client.transfer_from(&spender, &owner, &receiver, &200);

        assert_eq!(client.balance_of(&owner), 800);
        assert_eq!(client.balance_of(&receiver), 200);
        assert_eq!(client.allowance(&owner, &spender), 300);
    }

    #[test]
    #[should_panic(expected = "Insufficient allowance")]
    fn test_transfer_from_insufficient_allowance() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let owner = soroban_sdk::Address::generate(&env);
        let spender = soroban_sdk::Address::generate(&env);
        let receiver = soroban_sdk::Address::generate(&env);

        client.mint(&owner, &1_000);
        client.approve(&owner, &spender, &50);
        client.transfer_from(&spender, &owner, &receiver, &200);
    }

    #[test]
    fn test_burn() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let user = soroban_sdk::Address::generate(&env);
        client.mint(&user, &1_000);
        client.burn(&user, &400);
        assert_eq!(client.balance_of(&user), 600);
    }
}
