use crate::test::helpers::{setup_test_env, TestEnv};
use crate::staking::{StakingContract, RewardPool};

#[test]
fn rewards_accumulate_over_time() {
    let mut env: TestEnv = setup_test_env();
    let staker = env.create_account("staker_a", 1_000_000);
    env.stake(&staker, 500_000);

    env.deposit_fees(10_000);
    env.advance_time_days(7);

    let pending_week1 = env.pending_rewards(&staker);
    assert!(pending_week1 > 0, "rewards must accumulate after fee deposits");

    env.deposit_fees(10_000);
    env.advance_time_days(7);

    let pending_week2 = env.pending_rewards(&staker);
    assert!(
        pending_week2 > pending_week1,
        "rewards must keep accumulating across weeks"
    );
}

#[test]
fn claim_rewards_successfully() {
    let mut env: TestEnv = setup_test_env();
    let staker = env.create_account("staker_b", 1_000_000);
    env.stake(&staker, 250_000);

    env.deposit_fees(5_000);
    env.advance_time_days(7);

    let expected = env.pending_rewards(&staker);
    let balance_before = env.balance_of(&staker);

    let claimed = env.claim_rewards(&staker).expect("weekly claim must succeed");

    assert_eq!(claimed, expected, "claimed amount must equal pending rewards");
    assert_eq!(
        env.balance_of(&staker),
        balance_before + claimed,
        "claimed rewards must be transferred to staker"
    );
    assert_eq!(env.pending_rewards(&staker), 0, "pending must reset after claim");
    assert!(
        env.events().contains_reward_claimed(&staker, claimed),
        "RewardClaimed event must be emitted"
    );
}

#[test]
fn rewards_distributed_proportionally() {
    let mut env: TestEnv = setup_test_env();
    let alice = env.create_account("alice", 1_000_000);
    let bob = env.create_account("bob", 1_000_000);
    env.stake(&alice, 300_000);
    env.stake(&bob, 100_000);

    env.deposit_fees(40_000);
    env.advance_time_days(7);

    let alice_rewards = env.pending_rewards(&alice);
    let bob_rewards = env.pending_rewards(&bob);

    // Alice holds 3/4 of total stake, Bob holds 1/4.
    assert_eq!(alice_rewards, 30_000);
    assert_eq!(bob_rewards, 10_000);
    assert_eq!(alice_rewards + bob_rewards, 40_000);
}

#[test]
fn unstake_does_not_lose_rewards() {
    let mut env: TestEnv = setup_test_env();
    let staker = env.create_account("staker_c", 1_000_000);
    env.stake(&staker, 200_000);

    env.deposit_fees(8_000);
    env.advance_time_days(7);

    let accrued = env.pending_rewards(&staker);
    env.unstake(&staker, 200_000).expect("unstake must succeed");

    assert_eq!(
        env.pending_rewards(&staker),
        accrued,
        "unstaking must not forfeit accrued rewards"
    );

    let claimed = env.claim_rewards(&staker).expect("claim after unstake must succeed");
    assert_eq!(claimed, accrued);
}
