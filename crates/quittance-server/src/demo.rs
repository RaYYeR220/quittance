//! The live demo: a real mainnet wallet that answers any challenge with a fresh
//! disclosure of one of its past payments.

use serde_json::Value;

use crate::chain::Chain;

pub struct Demo {}

impl Demo {
    pub fn info(&self) -> Value {
        Value::Null
    }

    pub async fn prove(&self, _chain: &Chain, _msg: Vec<u8>, _payment: usize) -> Result<Value, String> {
        Err("not ready".into())
    }

    pub async fn forge_as_recipient(&self, _chain: &Chain, _msg: Vec<u8>, _payment: usize) -> Result<Value, String> {
        Err("not ready".into())
    }
}
