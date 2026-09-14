use async_trait::async_trait;
use helper_protocol::Request;

use crate::{
    infrastructure::helper::HelperClient,
    modules::event::awd::{AwdError, AwdResult, domain::firewall_state::DesiredFirewallState},
};

use super::{
    FirewallApplyResult, FirewallRuntime, FirewallVerification, ObservedFirewallState, TABLE_NAME,
    render,
};

/// 通过 `floatctf-helper` 应用/观测 FloatCTF 自有 nftables table。
pub struct HelperFirewallRuntime {
    client: HelperClient,
}

impl HelperFirewallRuntime {
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            client: HelperClient::new(socket_path),
        }
    }

    fn network_error(err: anyhow::Error) -> AwdError {
        AwdError::Network(err.to_string())
    }

    async fn list_table(&self) -> AwdResult<String> {
        self.client
            .call_data(Request::ListNftTable {
                table: TABLE_NAME.to_string(),
            })
            .await
            .map_err(Self::network_error)
    }
}

#[async_trait]
impl FirewallRuntime for HelperFirewallRuntime {
    async fn inspect(&self) -> AwdResult<ObservedFirewallState> {
        let out = self.list_table().await?;
        let mut state = render::parse_observed_table(&out);
        if !state.table_exists && !out.trim().is_empty() {
            state
                .notes
                .push("helper returned output without floatctf_awd table".into());
        }
        Ok(state)
    }

    async fn reconcile(&self, desired: &DesiredFirewallState) -> AwdResult<FirewallApplyResult> {
        if desired.is_empty() {
            self.client
                .call_empty(Request::DeleteNftTable {
                    table: TABLE_NAME.to_string(),
                })
                .await
                .map_err(Self::network_error)?;
            return Ok(FirewallApplyResult {
                revision: desired.revision,
                applied: true,
            });
        }

        let ruleset = render::render_table(desired);
        self.client
            .call_empty(Request::ApplyNftTable {
                table: TABLE_NAME.to_string(),
                ruleset,
                ensure_bridge_netfilter: true,
            })
            .await
            .map_err(Self::network_error)?;

        let verified = self.verify(desired).await?;
        if !verified.verified {
            return Err(AwdError::Network(format!(
                "helper firewall reconcile verify failed: {}",
                verified.notes.join("; ")
            )));
        }

        Ok(FirewallApplyResult {
            revision: desired.revision,
            applied: true,
        })
    }

    async fn verify(&self, desired: &DesiredFirewallState) -> AwdResult<FirewallVerification> {
        let out = self.list_table().await?;
        let observed = render::parse_observed_table(&out);
        let mut notes = Vec::new();

        if !observed.table_exists {
            notes.push("table inet floatctf_awd missing".into());
        }
        if observed.observed_revision != Some(desired.revision) {
            notes.push(format!(
                "revision mismatch: desired={} observed={:?}",
                desired.revision, observed.observed_revision
            ));
        }
        for event in &desired.events {
            let chain = format!(
                "event_{}",
                render::NftObjectName::event_key(&event.event_id).as_str()
            );
            if !observed.event_chains.contains(&chain) {
                notes.push(format!("missing chain {chain}"));
            }
        }

        Ok(FirewallVerification {
            verified: notes.is_empty(),
            observed,
            notes,
        })
    }
}
