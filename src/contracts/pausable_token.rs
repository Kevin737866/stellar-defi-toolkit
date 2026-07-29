//! Pausable token with transfer hooks (issues #22 + #221)
//!
//! Extends the basic pausable transfer mechanism with before/after transfer
//! hooks that external contracts can register.
//!
//! ## Access Control
//! - **Admin**: `pause`, `unpause`, `register_before_hook`,
//!   `register_after_hook`, `unregister_before_hook`, `unregister_after_hook`
//!   — enforced via `require_auth()` plus a stored-admin equality check.
//! - **User**: `transfer` — enforced via `from.require_auth()`.
//! - **Anyone**: `is_paused`, balance queries.

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Vec};

// ─── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum PausableKey {
    Paused,
    Admin,
    Balance(Address),
    /// Vec<Address> of before_transfer hook contracts (issue #221).
    BeforeHooks,
    /// Vec<Address> of after_transfer hook contracts (issue #221).
    AfterHooks,
    /// Map: hook_address -> blocks (bool).  `true` = hook is blocking.
    HookBlocks(Address),
}

// Maximum number of hooks to bound gas cost.
const MAX_HOOKS: u32 = 10;

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct PausableTokenContract;

#[contractimpl]
impl PausableTokenContract {
    // ── Lifecycle ─────────────────────────────────────────────────────────────

    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&PausableKey::Admin, &admin);
        env.storage().instance().set(&PausableKey::Paused, &false);

        let before_hooks: Vec<Address> = Vec::new(&env);
        env.storage().instance().set(&PausableKey::BeforeHooks, &before_hooks);

        let after_hooks: Vec<Address> = Vec::new(&env);
        env.storage().instance().set(&PausableKey::AfterHooks, &after_hooks);
    }

    // ── Pause Controls ────────────────────────────────────────────────────────

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
        env.storage()
            .instance()
            .get(&PausableKey::Paused)
            .unwrap_or(false)
    }

    // ── Transfer ──────────────────────────────────────────────────────────────

    /// Transfer tokens, executing registered hooks before and after.
    ///
    /// * **before_transfer** hooks: if any hook is in blocking mode the entire
    ///   transfer is reverted (`panic!`).
    /// * **after_transfer** hooks: fired for notification only; never revert.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();

        let paused: bool = env
            .storage()
            .instance()
            .get(&PausableKey::Paused)
            .unwrap_or(false);
        assert!(!paused, "transfers are paused");
        assert!(amount > 0, "amount must be positive");

        let from_balance: i128 = env
            .storage()
            .instance()
            .get(&PausableKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(from_balance >= amount, "insufficient balance");

        // Run before_transfer hooks — any blocking hook reverts the transfer.
        Self::run_before_hooks(&env, &from, &to, amount);

        // Perform balance update.
        let to_balance: i128 = env
            .storage()
            .instance()
            .get(&PausableKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&PausableKey::Balance(from.clone()), &(from_balance - amount));
        env.storage()
            .instance()
            .set(&PausableKey::Balance(to.clone()), &(to_balance + amount));

        // Emit Transfer event.
        env.events().publish(
            (symbol_short!("transfer"), from.clone(), to.clone()),
            amount,
        );

        // Run after_transfer hooks — notification only.
        Self::run_after_hooks(&env, &from, &to, amount);
    }

    // ── Hook Management (admin only) ──────────────────────────────────────────

    /// Register a before_transfer hook contract.
    pub fn register_before_hook(env: Env, admin: Address, hook: Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");

        let mut hooks: Vec<Address> = env
            .storage()
            .instance()
            .get(&PausableKey::BeforeHooks)
            .unwrap_or_else(|| Vec::new(&env));

        assert!(hooks.len() < MAX_HOOKS, "hook limit reached");

        // Reject duplicate registration.
        for h in hooks.iter() {
            if h == hook {
                panic!("hook already registered");
            }
        }

        hooks.push_back(hook.clone());
        env.storage().instance().set(&PausableKey::BeforeHooks, &hooks);

        // Default: hook is not blocking.
        env.storage()
            .instance()
            .set(&PausableKey::HookBlocks(hook.clone()), &false);

        env.events()
            .publish((symbol_short!("hook_reg"), symbol_short!("before")), hook);
    }

    /// Register an after_transfer hook contract.
    pub fn register_after_hook(env: Env, admin: Address, hook: Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");

        let mut hooks: Vec<Address> = env
            .storage()
            .instance()
            .get(&PausableKey::AfterHooks)
            .unwrap_or_else(|| Vec::new(&env));

        assert!(hooks.len() < MAX_HOOKS, "hook limit reached");

        for h in hooks.iter() {
            if h == hook {
                panic!("hook already registered");
            }
        }

        hooks.push_back(hook.clone());
        env.storage().instance().set(&PausableKey::AfterHooks, &hooks);

        env.events()
            .publish((symbol_short!("hook_reg"), symbol_short!("after")), hook);
    }

    /// Remove a before_transfer hook.
    pub fn unregister_before_hook(env: Env, admin: Address, hook: Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");

        let hooks: Vec<Address> = env
            .storage()
            .instance()
            .get(&PausableKey::BeforeHooks)
            .unwrap_or_else(|| Vec::new(&env));

        let mut new_hooks = Vec::new(&env);
        let mut found = false;
        for h in hooks.iter() {
            if h == hook {
                found = true;
            } else {
                new_hooks.push_back(h);
            }
        }
        assert!(found, "hook not registered");

        env.storage().instance().set(&PausableKey::BeforeHooks, &new_hooks);
        env.storage().instance().remove(&PausableKey::HookBlocks(hook));
    }

    /// Remove an after_transfer hook.
    pub fn unregister_after_hook(env: Env, admin: Address, hook: Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");

        let hooks: Vec<Address> = env
            .storage()
            .instance()
            .get(&PausableKey::AfterHooks)
            .unwrap_or_else(|| Vec::new(&env));

        let mut new_hooks = Vec::new(&env);
        let mut found = false;
        for h in hooks.iter() {
            if h == hook {
                found = true;
            } else {
                new_hooks.push_back(h);
            }
        }
        assert!(found, "hook not registered");

        env.storage().instance().set(&PausableKey::AfterHooks, &new_hooks);
    }

    /// Configure whether a before_transfer hook blocks transfers.
    ///
    /// This simulates a cross-contract call in tests.  In production a
    /// registered hook contract would be invoked; here we store the blocking
    /// flag on-chain so tests can exercise both paths.
    pub fn set_before_hook_blocks(env: Env, admin: Address, hook: Address, blocks: bool) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");
        env.storage()
            .instance()
            .set(&PausableKey::HookBlocks(hook), &blocks);
    }

    // ── Balance (helpers) ─────────────────────────────────────────────────────

    /// Mint tokens to an address (admin only).
    pub fn mint(env: Env, admin: Address, to: Address, amount: i128) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&PausableKey::Admin).unwrap();
        assert!(admin == stored, "unauthorized");
        assert!(amount > 0, "amount must be positive");

        let bal: i128 = env
            .storage()
            .instance()
            .get(&PausableKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&PausableKey::Balance(to), &(bal + amount));
    }

    pub fn balance(env: Env, addr: Address) -> i128 {
        env.storage()
            .instance()
            .get(&PausableKey::Balance(addr))
            .unwrap_or(0)
    }

    // ── Internal Hook Runners ─────────────────────────────────────────────────

    fn run_before_hooks(env: &Env, from: &Address, to: &Address, amount: i128) {
        let hooks: Vec<Address> = env
            .storage()
            .instance()
            .get(&PausableKey::BeforeHooks)
            .unwrap_or_else(|| Vec::new(env));

        for hook in hooks.iter() {
            let blocks: bool = env
                .storage()
                .instance()
                .get(&PausableKey::HookBlocks(hook.clone()))
                .unwrap_or(false);

            assert!(
                !blocks,
                "transfer rejected by before_transfer hook"
            );

            // In production, a cross-contract call would go here.
            env.events().publish(
                (symbol_short!("bf_hook"), hook.clone()),
                (from.clone(), to.clone(), amount),
            );
        }
    }

    fn run_after_hooks(env: &Env, from: &Address, to: &Address, amount: i128) {
        let hooks: Vec<Address> = env
            .storage()
            .instance()
            .get(&PausableKey::AfterHooks)
            .unwrap_or_else(|| Vec::new(env));

        for hook in hooks.iter() {
            // Notification only — never panic.
            env.events().publish(
                (symbol_short!("af_hook"), hook),
                (from.clone(), to.clone(), amount),
            );
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, soroban_sdk::Address, soroban_sdk::Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, PausableTokenContract);
        let admin = Address::generate(&env);
        PausableTokenContractClient::new(&env, &contract_id).initialize(&admin);
        (env, contract_id, admin)
    }

    // ── Pause / Unpause ───────────────────────────────────────────────────────

    #[test]
    fn test_pause_and_unpause() {
        let (env, contract_id, admin) = setup();
        let client = PausableTokenContractClient::new(&env, &contract_id);

        client.pause(&admin);
        assert!(client.is_paused());

        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    #[should_panic(expected = "transfers are paused")]
    fn test_transfer_fails_when_paused() {
        let (env, contract_id, admin) = setup();
        let client = PausableTokenContractClient::new(&env, &contract_id);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        client.mint(&admin, &from, &1000);
        client.pause(&admin);
        client.transfer(&from, &to, &100);
    }

    // ── Before-Transfer Hook ──────────────────────────────────────────────────

    #[test]
    fn test_before_hook_allows_transfer() {
        let (env, contract_id, admin) = setup();
        let client = PausableTokenContractClient::new(&env, &contract_id);
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        client.mint(&admin, &from, &1_000);
        client.register_before_hook(&admin, &hook);

        // Hook defaults to non-blocking → transfer succeeds.
        client.transfer(&from, &to, &400);
        assert_eq!(client.balance(&from), 600);
        assert_eq!(client.balance(&to), 400);
    }

    #[test]
    #[should_panic(expected = "transfer rejected by before_transfer hook")]
    fn test_before_hook_blocks_transfer() {
        let (env, contract_id, admin) = setup();
        let client = PausableTokenContractClient::new(&env, &contract_id);
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        client.mint(&admin, &from, &1_000);
        client.register_before_hook(&admin, &hook);
        client.set_before_hook_blocks(&admin, &hook, &true);

        // Hook is blocking → transfer reverts.
        client.transfer(&from, &to, &200);
    }

    #[test]
    fn test_after_hook_does_not_revert() {
        let (env, contract_id, admin) = setup();
        let client = PausableTokenContractClient::new(&env, &contract_id);
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        client.mint(&admin, &from, &1_000);
        client.register_after_hook(&admin, &hook);

        // After-transfer hook must never revert the transfer.
        client.transfer(&from, &to, &300);
        assert_eq!(client.balance(&from), 700);
        assert_eq!(client.balance(&to), 300);
    }

    #[test]
    fn test_unregister_before_hook_allows_transfer() {
        let (env, contract_id, admin) = setup();
        let client = PausableTokenContractClient::new(&env, &contract_id);
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        client.mint(&admin, &from, &1_000);
        client.register_before_hook(&admin, &hook);
        client.set_before_hook_blocks(&admin, &hook, &true);

        client.unregister_before_hook(&admin, &hook);

        // After unregistering, transfer must succeed.
        client.transfer(&from, &to, &100);
        assert_eq!(client.balance(&to), 100);
    }
}
