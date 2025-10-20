use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use pranklin_tx::{B256, Transaction};

/// HTTP client for sending requests to the RPC endpoint
pub struct RpcClient {
    client: Client,
    base_url: String,
}

impl RpcClient {
    /// Create a new RPC client
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(100)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    /// Submit a transaction
    pub async fn submit_transaction(&self, tx: &Transaction) -> Result<SubmitTxResponse> {
        let encoded = tx.encode();
        let hex_tx = format!("0x{}", hex::encode(encoded));

        let request = SubmitTxRequest { tx: hex_tx };

        let url = format!("{}/tx/submit", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        let result: SubmitTxResponse = response.json().await?;
        Ok(result)
    }

    /// Get transaction status
    #[allow(dead_code)]
    pub async fn get_transaction_status(&self, tx_hash: B256) -> Result<TxStatus> {
        let request = GetTxStatusRequest { tx_hash };

        let url = format!("{}/tx/status", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        let result: TxStatus = response.json().await?;
        Ok(result)
    }

    /// Get balance
    pub async fn get_balance(
        &self,
        address: alloy_primitives::Address,
        asset_id: u32,
    ) -> Result<GetBalanceResponse> {
        let request = GetBalanceRequest { address, asset_id };

        let url = format!("{}/account/balance", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        let result: GetBalanceResponse = response.json().await?;
        Ok(result)
    }

    /// Health check
    pub async fn health(&self) -> Result<String> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?.error_for_status()?;
        let text = response.text().await?;
        Ok(text)
    }
}

// Request/Response types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTxRequest {
    pub tx: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTxResponse {
    pub tx_hash: B256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct GetTxStatusRequest {
    pub tx_hash: B256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TxStatus {
    pub tx_hash: B256,
    pub status: String,
    pub block_height: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceRequest {
    pub address: alloy_primitives::Address,
    pub asset_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceResponse {
    pub balance: u128,
}
