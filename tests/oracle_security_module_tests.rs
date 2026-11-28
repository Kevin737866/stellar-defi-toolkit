//! Tests for the Oracle Security Module price delay and freeze behavior.

#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env};
    use stellar_defi_toolkit::contracts::oracle_manager::{
        OracleManagerContract, OracleManagerContractClient,
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

    #[test]
    fn price_becomes_current_only_after_configured_delay() {
        let (env, client, _) = setup();
        let asset = Address::generate(&env);
        env.ledger().set_timestamp(100);

        client.set_osm_delay(&asset, &10);
        client.queue_osm_price(&asset, &1_250_000, &6);

        let queued = client.get_osm_price(&asset);
        assert_eq!(queued.current_price, None);
        assert_eq!(queued.next_price, Some(1_250_000));
        assert_eq!(queued.next_update_at, 110);

        env.ledger().set_timestamp(109);
        assert_eq!(client.get_osm_price(&asset).current_price, None);

        env.ledger().set_timestamp(110);
        let applied = client.get_osm_price(&asset);
        assert_eq!(applied.current_price, Some(1_250_000));
        assert_eq!(applied.current_decimals, 6);
        assert_eq!(applied.current_updated_at, 110);
        assert_eq!(applied.next_price, None);
    }

    #[test]
    fn delay_is_configurable_independently_per_asset() {
        let (env, client, _) = setup();
        let fast_asset = Address::generate(&env);
        let slow_asset = Address::generate(&env);
        env.ledger().set_timestamp(100);

        client.set_osm_delay(&fast_asset, &10);
        client.set_osm_delay(&slow_asset, &20);
        client.queue_osm_price(&fast_asset, &2_000_000, &6);
        client.queue_osm_price(&slow_asset, &3_000_000, &6);

        env.ledger().set_timestamp(110);
        assert_eq!(
            client.get_osm_price(&fast_asset).current_price,
            Some(2_000_000)
        );
        assert_eq!(client.get_osm_price(&slow_asset).current_price, None);

        env.ledger().set_timestamp(120);
        assert_eq!(
            client.get_osm_price(&slow_asset).current_price,
            Some(3_000_000)
        );
    }

    #[test]
    fn recurring_updates_do_not_short_circuit_each_prices_delay() {
        let (env, client, _) = setup();
        let asset = Address::generate(&env);
        env.ledger().set_timestamp(100);

        client.set_osm_delay(&asset, &10);
        client.queue_osm_price(&asset, &1_000_000, &6);
        env.ledger().set_timestamp(105);
        client.queue_osm_price(&asset, &2_000_000, &6);
        env.ledger().set_timestamp(109);
        client.queue_osm_price(&asset, &3_000_000, &6);

        env.ledger().set_timestamp(110);
        let first = client.get_osm_price(&asset);
        assert_eq!(first.current_price, Some(1_000_000));
        assert_eq!(first.next_price, Some(3_000_000));
        assert_eq!(first.next_update_at, 119);

        env.ledger().set_timestamp(118);
        assert_eq!(
            client.get_osm_price(&asset).current_price,
            Some(1_000_000)
        );

        env.ledger().set_timestamp(119);
        assert_eq!(
            client.get_osm_price(&asset).current_price,
            Some(3_000_000)
        );
    }

    #[test]
    fn freeze_holds_pending_price_until_unfrozen() {
        let (env, client, _) = setup();
        let asset = Address::generate(&env);
        env.ledger().set_timestamp(100);

        client.set_osm_delay(&asset, &10);
        client.queue_osm_price(&asset, &1_000_000, &6);
        env.ledger().set_timestamp(110);
        assert_eq!(
            client.get_osm_price(&asset).current_price,
            Some(1_000_000)
        );

        client.queue_osm_price(&asset, &1_500_000, &6);
        client.set_osm_frozen(&asset, &true);

        env.ledger().set_timestamp(120);
        let frozen = client.get_osm_price(&asset);
        assert_eq!(frozen.current_price, Some(1_000_000));
        assert_eq!(frozen.next_price, Some(1_500_000));
        assert!(frozen.frozen);

        client.set_osm_frozen(&asset, &false);
        let unfrozen = client.get_osm_price(&asset);
        assert_eq!(unfrozen.current_price, Some(1_500_000));
        assert_eq!(unfrozen.next_price, None);
    }
}
