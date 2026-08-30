//! Tests for oracle heartbeat monitoring and alerting (#194)

#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};
    use stellar_defi_toolkit::contracts::oracle_manager::{
        OracleManagerContract, OracleManagerContractClient, AggregationMethod, AggregationParams,
        HeartbeatConfig, HeartbeatStatus,
    };

    fn setup() -> (Env, OracleManagerContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OracleManagerContract);
        let client = OracleManagerContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    // ─── Heartbeat Configuration ────────────────────────────────────────────

    #[test]
    fn test_set_heartbeat_config() {
        let (env, client, admin) = setup();
        let oracle = Address::generate(&env);

        // Register oracle first
        let name = Symbol::new(&env, "oracle1");
        client.register_oracle(&oracle, &name, &2000);

        // Configure heartbeat: 60s interval, alert after 3 misses
        client.set_heartbeat_config(&oracle, &60, &3);

        // Verify config was set
        let config = client.get_heartbeat_config(&oracle);
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.interval_seconds, 60);
        assert_eq!(config.max_missed, 3);
        assert!(config.enabled);
    }

    #[test]
    fn test_set_heartbeat_zero_interval_panics() {
        let (env, client, admin) = setup();
        let oracle = Address::generate(&env);

        let name = Symbol::new(&env, "oracle1");
        client.register_oracle(&oracle, &name, &2000);

        // Zero interval should panic
        let result = client.try_set_heartbeat_config(&oracle, &0, &3);
        assert!(result.is_err());
    }

    #[test]
    fn test_record_heartbeat() {
        let (env, client, admin) = setup();
        let oracle = Address::generate(&env);

        let name = Symbol::new(&env, "oracle1");
        client.register_oracle(&oracle, &name, &2000);

        // Configure heartbeat
        client.set_heartbeat_config(&oracle, &60, &3);

        // Record heartbeat
        client.record_heartbeat(&oracle);

        // Verify status
        let status = client.get_heartbeat_status(&oracle);
        assert!(status.is_some());
        let status = status.unwrap();
        assert_eq!(status.missed_count, 0);
        assert!(!status.flagged);
    }

    #[test]
    fn test_heartbeat_recovered_after_miss() {
        let (env, client, admin) = setup();
        let oracle = Address::generate(&env);

        let name = Symbol::new(&env, "oracle1");
        client.register_oracle(&oracle, &name, &2000);

        // Configure with 1s interval
        client.set_heartbeat_config(&oracle, &1, &3);

        // Advance time past one interval
        env.ledger().set_timestamp(5);

        // Check heartbeats - should detect misses
        client.check_heartbeats();

        // Verify flagged
        let status = client.get_heartbeat_status(&oracle).unwrap();
        assert!(status.flagged);

        // Record heartbeat - should recover
        client.record_heartbeat(&oracle);

        let status = client.get_heartbeat_status(&oracle).unwrap();
        assert!(!status.flagged);
        assert_eq!(status.missed_count, 0);
    }

    // ─── Check Heartbeats ───────────────────────────────────────────────────

    #[test]
    fn test_check_heartbeats_no_configured() {
        let (env, client, _admin) = setup();

        // No oracles configured for heartbeat - should not panic
        client.check_heartbeats();
    }

    #[test]
    fn test_check_heartbeats_detects_misses() {
        let (env, client, admin) = setup();
        let oracle = Address::generate(&env);

        let name = Symbol::new(&env, "oracle1");
        client.register_oracle(&oracle, &name, &2000);

        // 1 second interval
        client.set_heartbeat_config(&oracle, &1, &3);

        // Advance 10 seconds (10 missed heartbeats)
        env.ledger().set_timestamp(10);

        client.check_heartbeats();

        let status = client.get_heartbeat_status(&oracle).unwrap();
        assert!(status.missed_count > 0);
        assert!(status.total_missed > 0);
    }

    #[test]
    fn test_check_heartbeats_downgrades_reputation() {
        let (env, client, admin) = setup();
        let oracle = Address::generate(&env);

        let name = Symbol::new(&env, "oracle1");
        client.register_oracle(&oracle, &name, &2000);

        // 1 second interval, alert after 3 misses
        client.set_heartbeat_config(&oracle, &1, &3);

        // Advance 30 seconds (many missed heartbeats)
        env.ledger().set_timestamp(30);

        client.check_heartbeats();

        // Reputation should have been downgraded
        // Oracle starts at 8000 reputation, check_heartbeats should downgrade it
        let info = client.get_oracle_info(&oracle);
        assert!(info.reputation < 8000, "Reputation should decrease after consecutive misses");
    }

    // ─── Get All Heartbeat Statuses ─────────────────────────────────────────

    #[test]
    fn test_get_all_heartbeat_statuses_empty() {
        let (env, client, _admin) = setup();
        let statuses = client.get_all_heartbeat_statuses();
        assert_eq!(statuses.len(), 0);
    }

    #[test]
    fn test_get_all_heartbeat_statuses() {
        let (env, client, admin) = setup();
        let oracle1 = Address::generate(&env);
        let oracle2 = Address::generate(&env);

        let name1 = Symbol::new(&env, "oracle1");
        let name2 = Symbol::new(&env, "oracle2");
        client.register_oracle(&oracle1, &name1, &2000);
        client.register_oracle(&oracle2, &name2, &2000);

        client.set_heartbeat_config(&oracle1, &60, &3);
        client.set_heartbeat_config(&oracle2, &120, &5);

        let statuses = client.get_all_heartbeat_statuses();
        assert_eq!(statuses.len(), 2);
    }

    // ─── Disable Heartbeat ──────────────────────────────────────────────────

    #[test]
    fn test_disable_heartbeat() {
        let (env, client, admin) = setup();
        let oracle = Address::generate(&env);

        let name = Symbol::new(&env, "oracle1");
        client.register_oracle(&oracle, &name, &2000);

        client.set_heartbeat_config(&oracle, &60, &3);

        // Disable
        client.disable_heartbeat(&oracle);

        let config = client.get_heartbeat_config(&oracle).unwrap();
        assert!(!config.enabled);

        // Advance time - check_heartbeats should not detect misses for disabled oracle
        env.ledger().set_timestamp(300);
        client.check_heartbeats();

        let status = client.get_heartbeat_status(&oracle).unwrap();
        assert_eq!(status.missed_count, 0, "Disabled oracle should not accumulate misses");
    }
}
