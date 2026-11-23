use soroban_sdk::{contracttype, Address};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Proposal(u64),
    TimelockDelay,
    Delegation(Address),
}