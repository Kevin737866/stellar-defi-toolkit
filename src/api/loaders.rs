use async_graphql::dataloader::Loader;
use std::collections::HashMap;
use crate::utils::StellarClient;
use crate::api::types::Operation;
use std::sync::Arc;
use async_trait::async_trait;

pub struct OperationLoader {
    pub client: Arc<StellarClient>,
}

#[async_trait]
impl Loader<String> for OperationLoader {
    type Value = Vec<Operation>;
    type Error = Arc<anyhow::Error>;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let mut results = HashMap::new();
        
        for key in keys {
            // In a better implementation, we would batch this if Horizon supported it
            // or fetch from a database. For now, we fetch per transaction.
            match self.client.get_operations(None, 50).await {
                Ok(ops) => {
                    let filtered_ops: Vec<Operation> = ops.into_iter()
                        .filter(|op| op.transaction_id == *key)
                        .collect();
                    results.insert(key.clone(), filtered_ops);
                }
                Err(e) => return Err(Arc::new(e)),
            }
        }
        
        Ok(results)
    }
}
