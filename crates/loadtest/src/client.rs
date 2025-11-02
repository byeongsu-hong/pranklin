use anyhow::Result;
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;
use pranklin_tx::{B256, Transaction};

/// HTTP client for sending requests to the RPC endpoint
pub struct RpcClient {
    client: Client,
    base_url: String,
}

trait RpcRequest: Serialize {}
trait RpcResponse: DeserializeOwned {}

impl<T: Serialize> RpcRequest for T {}
impl<T: DeserializeOwned> RpcResponse for T {}

impl RpcClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(100)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    async fn post_json<Req: RpcRequest, Res: RpcResponse>(
        &self,
        endpoint: &str,
        request: &Req,
    ) -> Result<Res> {
        let url = format!("{}{}", self.base_url, endpoint);
        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    pub async fn submit_transaction(&self, tx: &Transaction) -> Result<SubmitTxResponse> {
        let request = SubmitTxRequest {
            tx: format!("0x{}", hex::encode(tx.encode())),
        };
        self.post_json("/tx/submit", &request).await
    }

    #[allow(dead_code)]
    pub async fn get_transaction_status(&self, tx_hash: B256) -> Result<TxStatus> {
        self.post_json("/tx/status", &GetTxStatusRequest { tx_hash }).await
    }

    pub async fn get_balance(
        &self,
        address: alloy_primitives::Address,
        asset_id: u32,
    ) -> Result<GetBalanceResponse> {
        self.post_json("/account/balance", &GetBalanceRequest { address, asset_id }).await
    }

    pub async fn health(&self) -> Result<String> {
        Ok(self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubmitTxRequest {
    tx: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTxResponse {
    pub tx_hash: B256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetTxStatusRequest {
    tx_hash: B256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxStatus {
    pub tx_hash: B256,
    pub status: String,
    pub block_height: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetBalanceRequest {
    address: alloy_primitives::Address,
    asset_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceResponse {
    pub balance: u128,
}
