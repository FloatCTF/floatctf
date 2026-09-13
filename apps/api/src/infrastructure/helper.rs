use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use helper_protocol::{Request, Response};
use serde::de::DeserializeOwned;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

#[derive(Debug, Clone)]
pub struct HelperClient {
    socket_path: Arc<str>,
}

impl HelperClient {
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: Arc::from(socket_path.into()),
        }
    }

    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }

    pub async fn call(&self, request: Request) -> Result<Response> {
        let mut stream = UnixStream::connect(self.socket_path.as_ref())
            .await
            .with_context(|| format!("connect floatctf-helper at {}", self.socket_path))?;
        let mut encoded = serde_json::to_vec(&request).context("encode helper request")?;
        encoded.push(b'\n');
        stream
            .write_all(&encoded)
            .await
            .context("write helper request")?;
        stream.shutdown().await.context("shutdown helper request")?;

        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .await
            .context("read helper response")?;
        if line.is_empty() {
            return Err(anyhow!("floatctf-helper returned an empty response"));
        }
        let response: Response = serde_json::from_str(&line).context("decode helper response")?;
        if !response.ok {
            return Err(anyhow!(
                "floatctf-helper: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            ));
        }
        Ok(response)
    }

    pub async fn call_empty(&self, request: Request) -> Result<()> {
        self.call(request).await.map(|_| ())
    }

    pub async fn call_data<T: DeserializeOwned>(&self, request: Request) -> Result<T> {
        let response = self.call(request).await?;
        let data = response
            .data
            .ok_or_else(|| anyhow!("floatctf-helper response has no data"))?;
        serde_json::from_value(data).context("decode helper response data")
    }
}
