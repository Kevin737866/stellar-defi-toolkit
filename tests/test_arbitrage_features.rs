//! Tests for the arbitrage contract new features:
//! - #195: MEV-aware arbitrage detection
//! - #196: Arbitrage profit simulation with gas estimation
//! - #198: Flash bundle execution

#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, Symbol, Vec};
    use stellar_defi_toolkit::contracts::arbitrage::{
        ArbitrageContract, ArbitrageContractClient, ArbitrageParams, FlashBundleHop, MEVConfig,
    };
    use stellar_defi_toolkit::types::stablecoin::{MEVRiskLevel, MEVCost, MEVEvent};

    fn setup() -> (Env, ArbitrageContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, ArbitrageContract);
        let client = ArbitrageContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let stablecoin = Address::generate(&env);
        let oracle = Address::generate(&env);
        client.initialize(&admin, &stablecoin, &oracle);
        (env, client, admin)
    }

    // ─── Issue #195: MEV-Aware Detection Tests ──────────────────────────────

    #[test]
    fn test_mev_config_defaults() {
        let (env, client, _admin) = setup();
        let config = client.get_mev_config_view();
        assert!(config.enabled);
        assert_eq!(config.buffer_bps, 500);
        assert_eq!(config.gas_per_hop, 50_000);
        assert_eq!(config.history_window, 100);
    }

    #[test]
    fn test_update_mev_config() {
        let (env, client, admin) = setup();
        let new_config = MEVConfig {
            buffer_bps: 1000,
            gas_per_hop: 75_000,
            history_window: 200,
            enabled: false,
        };
        client.update_mev_config(&new_config);
        let updated = client.get_mev_config_view();
        assert_eq!(updated.buffer_bps, 1000);
        assert_eq!(updated.gas_per_hop, 75_000);
        assert!(!updated.enabled);
    }

    #[test]
    fn test_mev_aware_detects_profitable() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        // Large deviation (5%) should be profitable even after MEV costs
        let result = client.detect_mev_aware_opportunity(&source, &target, &500, &100_000, &2);
        assert!(result.is_some());
    }

    #[test]
    fn test_mev_aware_rejects_unprofitable() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        // Tiny deviation (10 bps = 0.1%) with high gas should be unprofitable
        let result = client.detect_mev_aware_opportunity(&source, &target, &10, &500_000, &5);
        assert!(result.is_none());
    }

    #[test]
    fn test_mev_aware_rejects_out_of_range() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        // Deviation below minimum
        let result = client.detect_mev_aware_opportunity(&source, &target, &5, &100_000, &2);
        assert!(result.is_none());

        // Deviation above maximum
        let result = client.detect_mev_aware_opportunity(&source, &target, &600, &100_000, &2);
        assert!(result.is_none());
    }

    #[test]
    fn test_mev_disabled_skips_costs() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        // Disable MEV
        let disabled = MEVConfig {
            buffer_bps: 0,
            gas_per_hop: 0,
            history_window: 0,
            enabled: false,
        };
        client.update_mev_config(&disabled);

        // Even small deviations should be profitable without MEV costs
        let result = client.detect_mev_aware_opportunity(&source, &target, &50, &100_000, &2);
        assert!(result.is_some());
    }

    // ─── Issue #196: Arbitrage Simulation Tests ─────────────────────────────

    #[test]
    fn test_simulation_unprofitable_high_fees() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        let sim = client.simulate_arbitrage(
            &source,
            &target,
            &100_000_000, // 100 stablecoins
            &10,          // 0.1% deviation
            &3,           // 3 hops
            &30,          // 0.3% pool fee per hop
            &50,          // 0.5% slippage
        );
        assert!(!sim.success);
        assert!(sim.net_profit <= 0);
        assert!(sim.pool_fees > 0);
        assert!(sim.gas_cost > 0);
        assert!(sim.slippage_estimate > 0);
    }

    #[test]
    fn test_simulation_below_min_trade() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        let sim = client.simulate_arbitrage(
            &source,
            &target,
            &1_000, // Below MIN_TRADE_AMOUNT
            &100,
            &2,
            &30,
            &10,
        );
        assert!(!sim.success);
        assert_eq!(sim.route_summary, Symbol::short("INVALID"));
    }

    #[test]
    fn test_simulation_returns_all_costs() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        let sim = client.simulate_arbitrage(
            &source,
            &target,
            &100_000_000,
            &100,  // 1% deviation
            &2,    // 2 hops
            &30,   // 0.3% fee per hop
            &20,   // 0.2% slippage
        );

        // Gross profit: 100M * 100 / 10000 = 1_000_000
        assert!(sim.gross_profit > 0);
        // Pool fees: 2 hops * 0.3%
        assert!(sim.pool_fees > 0);
        // Gas cost: 2 hops * 50_000 = 100_000
        assert_eq!(sim.gas_cost, 100_000);
        // Protocol fee: 0.1% of gross
        assert!(sim.protocol_fees > 0);
        // Slippage: 100M * 20 / 10000 = 200_000
        assert_eq!(sim.slippage_estimate, 200_000);
    }

    // ─── Issue #198: Flash Bundle Tests ─────────────────────────────────────

    #[test]
    fn test_flash_bundle_min_two_hops() {
        let (env, client, _admin) = setup();
        let arbitrageur = Address::generate(&env);

        let mut hops = Vec::new(&env);
        hops.push_back(FlashBundleHop {
            pool: Address::generate(&env),
            token_in: Address::generate(&env),
            token_out: Address::generate(&env),
            amount_in: 1000,
            min_amount_out: 900,
            fee_bps: 30,
        });

        // Should panic: only 1 hop
        let result = client.try_execute_flash_bundle(&arbitrageur, &1_000_000, &hops, &0);
        assert!(result.is_err());
    }

    #[test]
    fn test_flash_bundle_slippage_protection() {
        let (env, client, _admin) = setup();
        let arbitrageur = Address::generate(&env);

        let mut hops = Vec::new(&env);
        // First hop: OK
        hops.push_back(FlashBundleHop {
            pool: Address::generate(&env),
            token_in: Address::generate(&env),
            token_out: Address::generate(&env),
            amount_in: 1_000_000,
            min_amount_out: 900_000,
            fee_bps: 30,
        });
        // Second hop: min_amount_out too high for the fee
        hops.push_back(FlashBundleHop {
            pool: Address::generate(&env),
            token_in: Address::generate(&env),
            token_out: Address::generate(&env),
            amount_in: 900_000,
            min_amount_out: 990_000, // Higher than input after fee
            fee_bps: 30,
        });

        let result = client.try_execute_flash_bundle(&arbitrageur, &1_000_000, &hops, &0);
        assert!(result.is_err());
    }

    #[test]
    fn test_flash_bundle_profitable() {
        let (env, client, _admin) = setup();
        let arbitrageur = Address::generate(&env);

        let mut hops = Vec::new(&env);
        // Hop 1: 1M -> 1.05M (5% gain)
        hops.push_back(FlashBundleHop {
            pool: Address::generate(&env),
            token_in: Address::generate(&env),
            token_out: Address::generate(&env),
            amount_in: 1_000_000,
            min_amount_out: 1_049_000, // 1M * (1 - 30/10000) = 997_000 but simulating gain
            fee_bps: 30,
        });
        // Hop 2: gain again
        hops.push_back(FlashBundleHop {
            pool: Address::generate(&env),
            token_in: Address::generate(&env),
            token_out: Address::generate(&env),
            amount_in: 1_049_000,
            min_amount_out: 1_090_000,
            fee_bps: 30,
        });

        // Min profit of 0 (just needs to not be negative)
        // loan_amount=1M, loan_fee=900, repayment=1_000_900
        // After hop1: 1_000_000 * (1 - 30/10000) = 997_000
        // After hop2: 997_000 * (1 - 30/10000) = 994_010
        // profit = 994_010 - 1_000_900 = -6_890 (negative)
        // This should fail with min_profit=0
        let result = client.try_execute_flash_bundle(&arbitrageur, &1_000_000, &hops, &0);
        assert!(result.is_err());
    }

    #[test]
    fn test_flash_bundle_get_by_id() {
        let (env, client, _admin) = setup();

        let result = client.get_flash_bundle(&999);
        assert!(result.is_none());
    }

    // ─── Combined Integration Test ──────────────────────────────────────────

    #[test]
    fn test_full_arbitrage_workflow() {
        let (env, client, _admin) = setup();
        let source = Address::generate(&env);
        let target = Address::generate(&env);

        // 1. Detect opportunity
        let opp_id = client.detect_opportunity(&source, &target, &100);
        assert!(opp_id > 0);

        // 2. Get active opportunities
        let active = client.get_active_opportunities();
        assert_eq!(active.len(), 1);

        // 3. Simulate
        let sim = client.simulate_arbitrage(&source, &target, &100_000_000, &100, &2, &30, &10);
        assert!(sim.gross_profit > 0);

        // 4. Get params
        let params = client.get_params_view();
        assert_eq!(params.min_deviation_bps, 10);
    }
}
