use crate::api::resolvers::QueryRoot;
use async_graphql::{EmptyMutation, EmptySubscription, Schema};

pub type StellarSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn create_schema(client: crate::utils::StellarClient) -> StellarSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(client)
        .finish()
}
