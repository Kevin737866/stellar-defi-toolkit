use soroban_sdk::{testutils::Address as _, Address, Env, Symbol, Vec};
use stellar_defi_toolkit::contracts::synthetic_protocol::{
    SyntheticProtocolContract, SyntheticProtocolContractClient,
};
use stellar_defi_toolkit::types::synthetic::{
    AssetType, ExchangeRequest, MintRequest, RedemptionRequest, SyntheticAsset,
};

const PRICE_1_USD: u64 = 100_000_000; // 6 decimals

fn setup() -> (Env, SyntheticProtocolContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SyntheticProtocolContract);
    let contract = SyntheticProtocolContractClient::new(&env, &contract_id);
    // `require_admin` compares the *contract's own* address against the stored
    // admin (a pre-existing bug, see docs/ACCESS_CONTROL_MATRIX.md), so the tests
    // register the contract itself as admin to reach the admin-gated paths.
    let admin = contract_id.clone();
    contract.initialize(&admin);

    let oracle = Address::generate(&env);

    let asset = |id: u32, symbol: &str, fee_bps: u32| SyntheticAsset {
        asset_id: id,
        symbol: Symbol::new(&env, symbol),
        name: Symbol::new(&env, symbol),
        description: Symbol::new(&env, symbol),
        asset_type: AssetType::Crypto,
        oracle_address: oracle.clone(),
        min_collateral_ratio: 15_000, // 150%
        max_collateral_ratio: 100_000,
        minting_fee_bps: fee_bps,
        active: true,
        listed_at: env.ledger().timestamp(),
        total_supply: 0,
        total_collateral: 0,
    };

    // list_asset assigns sequential ids starting at 1
    contract.list_asset(&asset(1, "sBTC", 50));
    contract.list_asset(&asset(2, "sETH", 50));

    // Oracles can only report for assets that are already listed.
    contract.update_oracle_price(&oracle, &1, &PRICE_1_USD, &9500);
    contract.update_oracle_price(&oracle, &2, &PRICE_1_USD, &9500);

    (env, contract, admin, oracle)
}

fn mint_request(env: &Env, user: &Address, asset_id: u32, collateral: u64, synth: u64) -> MintRequest {
    MintRequest {
        user: user.clone(),
        asset_id,
        collateral_token: Address::generate(env),
        collateral_amount: collateral,
        synthetic_amount: synth,
    }
}

// ─── #154 minting ───────────────────────────────────────────────────────────

#[test]
fn mint_credits_synth_and_registers_debt() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    let receipt = contract.mint(&mint_request(&env, &user, 1, 1_500, 1_000));

    // 0.5% minting fee is withheld from the credited amount.
    assert_eq!(receipt.synthetic_minted, 995);
    assert_eq!(receipt.minting_fee, 5);
    assert_eq!(receipt.price, PRICE_1_USD);
    assert_eq!(receipt.collateral_ratio, 15_075);

    let position = contract.get_user_position(&user, &1);
    assert_eq!(position.synthetic_amount, 995);
    assert_eq!(position.debt_amount, 995, "only the credited amount is owed");
    assert_eq!(position.collateral_ratio, 15_075);
}

#[test]
fn mint_rejects_collateral_below_minimum_ratio() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.mint(&mint_request(&env, &user, 1, 1_400, 1_000));
    }));
    assert!(result.is_err(), "140% collateral must be rejected at a 150% minimum");
}

#[test]
fn mint_accepts_exactly_the_minimum_ratio() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    let receipt = contract.mint(&mint_request(&env, &user, 1, 1_500, 1_000));
    assert!(receipt.collateral_ratio >= 15_000, "ratio must clear the 150% minimum");
}

#[test]
fn mint_supports_multiple_collateral_types() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    let mut first = mint_request(&env, &user, 1, 900, 600);
    contract.mint(&first);

    first.collateral_token = Address::generate(&env);
    first.collateral_amount = 900;
    contract.mint(&first);

    let position = contract.get_user_position(&user, &1);
    assert_eq!(position.collateral_deposits.len(), 2, "both collateral tokens recorded");
    assert_eq!(position.synthetic_amount, 1_194);
    assert_eq!(position.debt_amount, 1_194);
}

#[test]
fn mint_accumulates_into_existing_position() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    contract.mint(&mint_request(&env, &user, 1, 1_500, 1_000));
    contract.mint(&mint_request(&env, &user, 1, 1_500, 1_000));

    let position = contract.get_user_position(&user, &1);
    assert_eq!(position.synthetic_amount, 1_990);
    assert_eq!(position.debt_amount, 1_990);

    let asset = contract.get_asset(&1);
    assert_eq!(asset.total_supply, 1_990);
    assert_eq!(asset.total_collateral, 3_000);
}

#[test]
fn mint_requires_oracle_confidence() {
    let (env, contract, _admin, oracle) = setup();

    // The feed itself refuses to publish a price below the confidence floor, so a
    // stale/low-confidence quote can never be used to mint.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.update_oracle_price(&oracle, &1, &PRICE_1_USD, &5000);
    }));
    assert!(result.is_err(), "sub-threshold confidence must be rejected");

    // The previously published 95% quote is still in force.
    let user = Address::generate(&env);
    let receipt = contract.mint(&mint_request(&env, &user, 1, 1_500, 1_000));
    assert_eq!(receipt.price, PRICE_1_USD);
}

#[test]
fn mint_tracks_price_changes() {
    let (env, contract, _admin, oracle) = setup();
    let user = Address::generate(&env);

    // Price doubles: the same nominal collateral is now worth more USD.
    contract.update_oracle_price(&oracle, &1, &(PRICE_1_USD * 2), &9500);
    let receipt = contract.mint(&mint_request(&env, &user, 1, 1_500, 1_000));

    assert_eq!(receipt.collateral_value_usd, 300_000);
    assert_eq!(receipt.debt_value_usd, 199_000, "debt is the 995 credited units");
}

// ─── #155 redemption ────────────────────────────────────────────────────────

fn redeem_request(_env: &Env, user: &Address, asset_id: u32, synth: u64, token: &Address) -> RedemptionRequest {
    RedemptionRequest {
        user: user.clone(),
        asset_id,
        synthetic_amount: synth,
        collateral_token: token.clone(),
    }
}

fn funded_position(env: &Env, contract: &SyntheticProtocolContractClient) -> (Address, Address) {
    let user = Address::generate(&env);
    let token = Address::generate(env);
    let mut request = mint_request(env, &user, 1, 2_000, 1_000);
    request.collateral_token = token.clone();
    contract.mint(&request);
    (user, token)
}

#[test]
fn redeem_returns_collateral_net_of_fee() {
    let (env, contract, _admin, _oracle) = setup();
    let (user, token) = funded_position(&env, &contract);

    let receipt = contract.redeem(&redeem_request(&env, &user, 1, 400, &token));

    // 0.5% default redemption fee on 400 units of collateral.
    assert_eq!(receipt.redemption_fee, 2);
    assert_eq!(receipt.collateral_released, 398);
    assert_eq!(receipt.synthetic_burned, 400);
    assert_eq!(receipt.price, PRICE_1_USD);

    let position = contract.get_user_position(&user, &1);
    assert_eq!(position.synthetic_amount, 595);
    assert_eq!(position.debt_amount, 595);
}

#[test]
fn redeem_supports_partial_redemption() {
    let (env, contract, _admin, _oracle) = setup();
    let (user, token) = funded_position(&env, &contract);

    contract.redeem(&redeem_request(&env, &user, 1, 100, &token));
    contract.redeem(&redeem_request(&env, &user, 1, 100, &token));

    let position = contract.get_user_position(&user, &1);
    assert_eq!(position.synthetic_amount, 795);
    assert_eq!(position.collateral_deposits.get(token).unwrap(), 1_800);
}

#[test]
fn redeem_full_position_clears_debt_and_leaves_surplus_collateral() {
    let (env, contract, _admin, _oracle) = setup();
    let (user, token) = funded_position(&env, &contract);

    // Burning the whole balance retires the debt; the collateral that was backing
    // it above the 150% floor is not released, so it stays deposited.
    contract.redeem(&redeem_request(&env, &user, 1, 995, &token));

    let position = contract.get_user_position(&user, &1);
    assert_eq!(position.synthetic_amount, 0);
    assert_eq!(position.debt_amount, 0);
    assert_eq!(position.collateral_deposits.get(token).unwrap(), 1_005);
    assert_eq!(position.collateral_ratio, 100_000, "no debt => max ratio");
}

#[test]
fn redeem_rejects_more_than_balance() {
    let (env, contract, _admin, _oracle) = setup();
    let (user, token) = funded_position(&env, &contract);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.redeem(&redeem_request(&env, &user, 1, 5_000, &token));
    }));
    assert!(result.is_err(), "cannot burn more synths than held");
}

#[test]
fn redemption_fee_is_configurable() {
    let (env, contract, _admin, _oracle) = setup();
    assert_eq!(contract.get_redemption_fee(), 50);

    contract.set_redemption_fee(&200);
    assert_eq!(contract.get_redemption_fee(), 200);

    let (user, token) = funded_position(&env, &contract);
    let receipt = contract.redeem(&redeem_request(&env, &user, 1, 400, &token));
    assert_eq!(receipt.redemption_fee, 8, "2% of 400");
    assert_eq!(receipt.collateral_released, 392);
}

#[test]
fn redemption_fee_above_max_is_rejected() {
    let (env, contract, _admin, _oracle) = setup();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.set_redemption_fee(&5_000);
    }));
    assert!(result.is_err());
}

// ─── #157 exchange ──────────────────────────────────────────────────────────

fn exchange_request(_env: &Env, user: &Address, from: u32, to: u32, amount: u64) -> ExchangeRequest {
    ExchangeRequest {
        user: user.clone(),
        from_asset_id: from,
        to_asset_id: to,
        synthetic_amount: amount,
    }
}

fn route(env: &Env, ids: &[u32]) -> Vec<u32> {
    let mut v = Vec::new(env);
    for id in ids {
        v.push_back(*id);
    }
    v
}

#[test]
fn exchange_converts_at_oracle_rate_and_charges_fee() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    contract.mint(&mint_request(&env, &user, 1, 3_000, 2_000));

    let receipt = contract.exchange(
        &exchange_request(&env, &user, 1, 2, 1_000),
        &route(&env, &[1, 2]),
    );

    // Equal prices, 0.3% default exchange fee.
    assert_eq!(receipt.exchange_fee, 3);
    assert_eq!(receipt.amount_out, 997);
    assert_eq!(receipt.hops, 1);

    let from_position = contract.get_user_position(&user, &1);
    assert_eq!(from_position.synthetic_amount, 990);
    assert_eq!(from_position.debt_amount, 990, "source debt reduced in step");

    let to_position = contract.get_user_position(&user, &2);
    assert_eq!(to_position.synthetic_amount, 997);
    assert_eq!(to_position.debt_amount, 997, "destination debt pool increased");
}

#[test]
fn exchange_respects_price_ratio() {
    let (env, contract, _admin, oracle) = setup();
    let user = Address::generate(&env);
    contract.mint(&mint_request(&env, &user, 1, 3_000, 2_000));

    // Destination synth is worth 4x the source.
    contract.update_oracle_price(&oracle, &2, &(PRICE_1_USD * 4), &9500);

    let receipt = contract.exchange(
        &exchange_request(&env, &user, 1, 2, 1_000),
        &route(&env, &[1, 2]),
    );

    // 1000 units * $1 / $4 = 250 units; the 0.3% fee truncates to 0 at this size.
    assert_eq!(receipt.exchange_fee, 0);
    assert_eq!(receipt.amount_out, 250);
    assert_eq!(receipt.from_price, PRICE_1_USD);
    assert_eq!(receipt.to_price, PRICE_1_USD * 4);
}

#[test]
fn exchange_supports_multi_hop_routing() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    contract.mint(&mint_request(&env, &user, 1, 3_000, 2_000));

    let receipt = contract.exchange(
        &exchange_request(&env, &user, 1, 2, 1_000),
        &route(&env, &[1, 1, 2]),
    );

    assert_eq!(receipt.hops, 2, "three-asset route performs two hops");
    // Fees truncate per hop: 1000 -> (fee 3) 997 -> (fee 2) 995.
    assert_eq!(receipt.amount_out, 995);
    assert_eq!(receipt.exchange_fee, 5);
}

#[test]
fn exchange_rejects_invalid_route() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    contract.mint(&mint_request(&env, &user, 1, 3_000, 2_000));

    let too_short = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.exchange(&exchange_request(&env, &user, 1, 2, 100), &route(&env, &[1]));
    }));
    assert!(too_short.is_err(), "single-asset route rejected");

    let mismatched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.exchange(&exchange_request(&env, &user, 1, 2, 100), &route(&env, &[2, 1]));
    }));
    assert!(mismatched.is_err(), "route must start at the source and end at the destination");
}

#[test]
fn exchange_rejects_insufficient_balance() {
    let (env, contract, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    contract.mint(&mint_request(&env, &user, 1, 3_000, 2_000));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.exchange(&exchange_request(&env, &user, 1, 2, 50_000), &route(&env, &[1, 2]));
    }));
    assert!(result.is_err());
}

#[test]
fn exchange_fee_is_configurable() {
    let (env, contract, _admin, _oracle) = setup();
    assert_eq!(contract.get_exchange_fee(), 30);

    contract.set_exchange_fee(&100);
    let user = Address::generate(&env);
    contract.mint(&mint_request(&env, &user, 1, 3_000, 2_000));

    let receipt = contract.exchange(
        &exchange_request(&env, &user, 1, 2, 1_000),
        &route(&env, &[1, 2]),
    );
    assert_eq!(receipt.exchange_fee, 10, "1% of 1000");
    assert_eq!(receipt.amount_out, 990);
}
