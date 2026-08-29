// contracts/escrow/src/lib.rs
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    pub sender: Address,
    pub recipient: Address,
    pub token: Address,
    pub amount: i128,
    pub hashlock: BytesN<32>,
    pub expiration: u64,
    pub claimed: bool,
    pub refunded: bool,
}

#[contracttype]
pub enum DataKey {
    Escrow(u64),
}

#[contract]
pub struct CrossContractEscrowContract;

#[contractimpl]
impl CrossContractEscrowContract {
    pub fn create_escrow(
        env: Env,
        sender: Address,
        recipient: Address,
        token: Address,
        escrow_id: u64,
        amount: i128,
        hashlock: BytesN<32>,
        duration: u64,
    ) {
        sender.require_auth();
        if amount <= 0 {
            panic!("Escrow amount must be positive");
        }

        let key = DataKey::Escrow(escrow_id);
        if env.storage().persistent().has(&key) {
            panic!("Escrow ID already exists");
        }

        let expiration = env.ledger().timestamp() + duration;

        let escrow = Escrow {
            sender: sender.clone(),
            recipient,
            token: token.clone(),
            amount,
            hashlock,
            expiration,
            claimed: false,
            refunded: false,
        };

        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (Symbol::new(&env, "EscrowCreated"), escrow_id),
            (sender, amount),
        );
    }

    pub fn claim(env: Env, recipient: Address, escrow_id: u64, preimage: BytesN<32>) {
        recipient.require_auth();

        let key = DataKey::Escrow(escrow_id);
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("Escrow not found"));

        if escrow.recipient != recipient {
            panic!("Unauthorized recipient");
        }

        if escrow.claimed || escrow.refunded {
            panic!("Escrow already settled");
        }

        if env.ledger().timestamp() > escrow.expiration {
            panic!("Escrow has expired");
        }

        let computed_hash = env.crypto().sha256(&preimage.into());
        if computed_hash != escrow.hashlock.into() {
            panic!("Invalid secret preimage");
        }

        escrow.claimed = true;
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (Symbol::new(&env, "EscrowClaimed"), escrow_id),
            recipient,
        );
    }

    pub fn refund(env: Env, sender: Address, escrow_id: u64) {
        sender.require_auth();

        let key = DataKey::Escrow(escrow_id);
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("Escrow not found"));

        if escrow.sender != sender {
            panic!("Unauthorized sender");
        }

        if escrow.claimed || escrow.refunded {
            panic!("Escrow already settled");
        }

        if env.ledger().timestamp() <= escrow.expiration {
            panic!("Escrow has not yet expired");
        }

        escrow.refunded = true;
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (Symbol::new(&env, "EscrowRefunded"), escrow_id),
            sender,
        );
    }
}