pub mod schema;
pub mod resolvers;
pub mod loaders;
pub mod aggregations;
pub mod types;
pub mod ws;

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
use crate::api::ws::PriceBroadcaster;

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
    let broadcaster = PriceBroadcaster::new();

    let app = Router::new()
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        .route("/ws/prices", get(ws::ws_handler))
        .layer(axum::Extension(broadcaster))
        .with_state(schema);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("Stellar Analytics GraphQL API starting on http://{}", addr);
    log::info!("GraphQL Playground available at http://{}/graphql", addr);
    log::info!("WebSocket price feed available at ws://{}/ws/prices", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
