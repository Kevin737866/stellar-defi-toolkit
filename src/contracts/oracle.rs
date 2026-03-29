use std::collections::BTreeMap;

use crate::types::ProtocolError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceOracle {
    admin: String,
    prices: BTreeMap<String, i128>,
}

impl PriceOracle {
    pub fn new(admin: impl Into<String>) -> Self {
        Self {
            admin: admin.into(),
            prices: BTreeMap::new(),
        }
    }

    pub fn admin(&self) -> &str {
        &self.admin
    }

    pub fn set_price(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        price: i128,
    ) -> Result<(), ProtocolError> {
        if caller != self.admin {
            return Err(ProtocolError::Unauthorized);
        }
        if price <= 0 {
            return Err(ProtocolError::InvalidAmount);
        }
        self.prices.insert(asset.into(), price);
        Ok(())
    }

    pub fn get_price(&self, asset: &str) -> Result<i128, ProtocolError> {
        self.prices
            .get(asset)
            .copied()
            .ok_or_else(|| ProtocolError::MissingPrice(asset.to_string()))
    }
}
