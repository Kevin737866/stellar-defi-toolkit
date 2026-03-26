pub mod schema;
pub mod resolvers;
pub mod loaders;
pub mod aggregations;
pub mod types;

use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use crate::utils::StellarClient;
use crate::api::schema::{create_schema, StellarSchema};

pub async fn graphql_handler(
    State(schema): State<StellarSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

pub async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

pub async fn start_api_server(port: u16, client: StellarClient) -> anyhow::Result<()> {
    let schema = create_schema(client);

    let app = Router::new()
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        .with_state(schema);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("Stellar Analytics GraphQL API starting on http://{}", addr);
    log::info!("GraphQL Playground available at http://{}/graphql", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
