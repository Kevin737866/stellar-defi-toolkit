#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ProposalCategory {
    Parameter,
    Upgrade,
    Treasury,
    Emergency,
}

pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub category: ProposalCategory,
    pub module: Option<String>,

    pub status: ProposalStatus,

    pub created_at: u64,

    pub voting_end: u64,

    pub queued_at: Option<u64>,

    pub executable_at: Option<u64>,
}