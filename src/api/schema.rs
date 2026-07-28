use crate::api::resolvers::QueryRoot;
use crate::contracts::price_history::PriceHistoryManager;
use async_graphql::{EmptyMutation, EmptySubscription, Schema};

pub type StellarSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn create_schema(
    client: crate::utils::StellarClient,
    price_manager: PriceHistoryManager,
) -> StellarSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(client)
        .data(price_manager)
        .finish()
}
