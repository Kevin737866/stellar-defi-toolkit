pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: String,
    pub description: String,

    pub status: ProposalStatus,

    pub created_at: u64,

    pub voting_end: u64,

    pub queued_at: Option<u64>,

    pub executable_at: Option<u64>,
}