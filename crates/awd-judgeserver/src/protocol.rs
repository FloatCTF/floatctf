//! AWD JudgeServer ↔ Platform API 协议类型与 URL 构建。
//!
//! 所有类型与 `apps/api/src/modules/event/awd/api/dto.rs` 中的 Wave 3 DTO 对齐。
//! 两边独立定义，通过 serde 字段名匹配（默认 snake_case）保证兼容。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Claim ──

#[derive(Debug, Clone, Serialize)]
pub struct JudgeClaimRequest {
    pub worker_id: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JudgeClaimResponse {
    pub tasks: Vec<ClaimedTask>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaimedTask {
    pub task_id: Uuid,
    #[allow(dead_code)]
    pub batch_id: Uuid,
    #[allow(dead_code)]
    pub event_id: Uuid,
    #[allow(dead_code)]
    pub round_id: Uuid,
    #[allow(dead_code)]
    pub gamebox_instance_id: Uuid,
    #[allow(dead_code)]
    pub event_gamebox_id: Option<Uuid>,
    #[allow(dead_code)]
    pub team_id: Uuid,
    pub attempt: i32,
    pub lease_token: String,
    #[allow(dead_code)]
    pub lease_expires_at: String,
    #[allow(dead_code)]
    pub deadline_at: String,
    // Execution payload
    pub script_content: String,
    pub script_args_json: Option<String>,
    pub target_ip: String,
    pub timeout_secs: i32,
}

// ── Heartbeat ──

#[derive(Debug, Clone, Serialize)]
pub struct JudgeHeartbeatRequest {
    pub worker_id: String,
    pub attempt: i32,
    pub lease_token: String,
}

// ── Result ──

#[derive(Debug, Clone, Serialize)]
pub struct JudgeResultRequest {
    pub worker_id: String,
    pub attempt: i32,
    pub lease_token: String,
    pub result_id: String,
    pub outcome: String,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<i32>,
}

// ── URL builders ──

pub fn claim_url(platform_url: &str, event_id: &str) -> String {
    format!("{}/internal/awd/events/{}/judge/claim", platform_url, event_id)
}

pub fn heartbeat_url(platform_url: &str, event_id: &str, task_id: &Uuid) -> String {
    format!(
        "{}/internal/awd/events/{}/judge/tasks/{}/heartbeat",
        platform_url, event_id, task_id
    )
}

pub fn result_url(platform_url: &str, event_id: &str, task_id: &Uuid) -> String {
    format!(
        "{}/internal/awd/events/{}/judge/tasks/{}/result",
        platform_url, event_id, task_id
    )
}

pub fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

// ── Cross-side serialization verification tests ──

#[cfg(test)]
mod tests {
    use super::*;

    // ── Claim ──

    #[test]
    fn claim_request_serializes_correctly() {
        let req = JudgeClaimRequest {
            worker_id: "w1".into(),
            limit: 5,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["worker_id"], "w1");
        assert_eq!(parsed["limit"], 5);
    }

    #[test]
    fn claim_response_deserializes_all_fields() {
        let json = r###"{
            "tasks": [{
                "task_id": "00000000-0000-0000-0000-000000000001",
                "batch_id": "00000000-0000-0000-0000-000000000002",
                "event_id": "00000000-0000-0000-0000-000000000003",
                "round_id": "00000000-0000-0000-0000-000000000004",
                "gamebox_instance_id": "00000000-0000-0000-0000-000000000005",
                "event_gamebox_id": null,
                "team_id": "00000000-0000-0000-0000-000000000006",
                "attempt": 1,
                "lease_token": "abc123",
                "lease_expires_at": "2026-01-01T00:00:00Z",
                "deadline_at": "2026-01-01T00:05:00Z",
                "script_content": "#!/bin/bash\necho ok",
                "script_args_json": "[\"check\",\"{target_ip}\"]",
                "target_ip": "10.0.0.1",
                "timeout_secs": 30
            }]
        }"###;
        let resp: JudgeClaimResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.tasks.len(), 1);
        let t = &resp.tasks[0];
        assert_eq!(t.task_id.to_string(), "00000000-0000-0000-0000-000000000001");
        assert_eq!(t.attempt, 1);
        assert_eq!(t.lease_token, "abc123");
        assert_eq!(t.script_content, "#!/bin/bash\necho ok");
        assert_eq!(t.script_args_json.as_deref(), Some("[\"check\",\"{target_ip}\"]"));
        assert_eq!(t.target_ip, "10.0.0.1");
        assert_eq!(t.timeout_secs, 30);
    }

    #[test]
    fn claim_response_deserializes_multiple_tasks() {
        let json = r###"{"tasks":[
            {"task_id":"00000000-0000-0000-0000-000000000001","batch_id":"00000000-0000-0000-0000-000000000002","event_id":"00000000-0000-0000-0000-000000000003","round_id":"00000000-0000-0000-0000-000000000004","gamebox_instance_id":"00000000-0000-0000-0000-000000000005","event_gamebox_id":null,"team_id":"00000000-0000-0000-0000-000000000006","attempt":1,"lease_token":"tok1","lease_expires_at":"2026-01-01T00:00:00Z","deadline_at":"2026-01-01T00:05:00Z","script_content":"echo 1","script_args_json":null,"target_ip":"10.0.0.1","timeout_secs":30},
            {"task_id":"00000000-0000-0000-0000-000000000007","batch_id":"00000000-0000-0000-0000-000000000008","event_id":"00000000-0000-0000-0000-000000000003","round_id":"00000000-0000-0000-0000-000000000004","gamebox_instance_id":"00000000-0000-0000-0000-000000000009","event_gamebox_id":null,"team_id":"00000000-0000-0000-0000-00000000000a","attempt":2,"lease_token":"tok2","lease_expires_at":"2026-01-01T00:00:00Z","deadline_at":"2026-01-01T00:05:00Z","script_content":"echo 2","script_args_json":null,"target_ip":"10.0.0.2","timeout_secs":30}
        ]}"###;
        let resp: JudgeClaimResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.tasks.len(), 2);
        assert_eq!(resp.tasks[0].lease_token, "tok1");
        assert_eq!(resp.tasks[1].lease_token, "tok2");
    }

    // ── Heartbeat ──

    #[test]
    fn heartbeat_request_serializes_correctly() {
        let req = JudgeHeartbeatRequest {
            worker_id: "w1".into(),
            attempt: 1,
            lease_token: "abc123".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["worker_id"], "w1");
        assert_eq!(parsed["attempt"], 1);
        assert_eq!(parsed["lease_token"], "abc123");
    }

    // ── Result (all outcome variants) ──

    #[test]
    fn result_request_up_serializes_correctly() {
        let req = JudgeResultRequest {
            worker_id: "w1".into(),
            attempt: 1,
            lease_token: "abc123".into(),
            result_id: "r1".into(),
            outcome: "up".into(),
            exit_code: Some(0),
            stdout: Some("OK".into()),
            stderr: None,
            duration_ms: Some(150),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["outcome"], "up");
        assert_eq!(parsed["exit_code"], 0);
        assert_eq!(parsed["stdout"], "OK");
        assert_eq!(parsed["duration_ms"], 150);
    }

    #[test]
    fn result_request_down_serializes_correctly() {
        let req = JudgeResultRequest {
            worker_id: "w1".into(),
            attempt: 1,
            lease_token: "abc123".into(),
            result_id: "r2".into(),
            outcome: "down".into(),
            exit_code: Some(1),
            stdout: None,
            stderr: Some("connection refused".into()),
            duration_ms: Some(2000),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["outcome"], "down");
        assert_eq!(parsed["exit_code"], 1);
        assert_eq!(parsed["stderr"], "connection refused");
    }

    #[test]
    fn result_request_target_timeout_serializes_correctly() {
        let req = JudgeResultRequest {
            worker_id: "w1".into(),
            attempt: 1,
            lease_token: "abc123".into(),
            result_id: "r3".into(),
            outcome: "target_timeout".into(),
            exit_code: None,
            stdout: None,
            stderr: Some("Execution timed out".into()),
            duration_ms: Some(30000),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["outcome"], "target_timeout");
        assert_eq!(parsed["exit_code"], serde_json::Value::Null);
    }

    #[test]
    fn result_request_worker_error_serializes_correctly() {
        let req = JudgeResultRequest {
            worker_id: "w1".into(),
            attempt: 1,
            lease_token: "abc123".into(),
            result_id: "r4".into(),
            outcome: "worker_error".into(),
            exit_code: None,
            stdout: None,
            stderr: Some("Script write error: Permission denied".into()),
            duration_ms: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["outcome"], "worker_error");
        assert_eq!(parsed["duration_ms"], serde_json::Value::Null);
    }

    // ── Cross-side: API → JudgeServer (ClaimResponse) ──

    #[test]
    fn api_claim_response_roundtrips_to_judgeserver() {
        // Simulate API serializing a ClaimResponse (using the API's DTO shape)
        let api_json = serde_json::json!({
            "tasks": [{
                "task_id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                "batch_id": "11111111-2222-3333-4444-555555555555",
                "event_id": "66666666-7777-8888-9999-000000000000",
                "round_id": "aaaaaaaa-1111-2222-3333-bbbbbbbbbbbb",
                "gamebox_instance_id": "cccccccc-1111-2222-3333-dddddddddddd",
                "event_gamebox_id": null,
                "team_id": "eeeeeeee-1111-2222-3333-ffffffffffff",
                "attempt": 1,
                "lease_token": "fa3c8e1b2d4a5f6c7e8d9a0b1c2d3e4f",
                "lease_expires_at": "2026-08-26T12:00:00Z",
                "deadline_at": "2026-08-26T12:05:00Z",
                "script_content": "#!/bin/bash\ncurl -sf http://{target_ip}:8080/health",
                "script_args_json": "[\"check\",\"{target_ip}\"]",
                "target_ip": "10.42.1.100",
                "timeout_secs": 30
            }]
        });
        let api_json_str = serde_json::to_string(&api_json).unwrap();

        // JudgeServer deserializes it
        let resp: JudgeClaimResponse = serde_json::from_str(&api_json_str).unwrap();
        assert_eq!(resp.tasks.len(), 1);
        let t = &resp.tasks[0];
        assert_eq!(t.lease_token, "fa3c8e1b2d4a5f6c7e8d9a0b1c2d3e4f");
        assert_eq!(t.attempt, 1);
        assert_eq!(t.target_ip, "10.42.1.100");
        assert_eq!(t.timeout_secs, 30);
        assert!(t.script_content.contains("curl"));
    }

    // ── Cross-side: JudgeServer → API (HeartbeatRequest) ──

    #[test]
    fn judgeserver_heartbeat_roundtrips_to_api() {
        let req = JudgeHeartbeatRequest {
            worker_id: "worker-uuid-1234".into(),
            attempt: 2,
            lease_token: "hex-lease-token".into(),
        };
        let json = serde_json::to_string(&req).unwrap();

        // API would deserialize this as JudgeHeartbeatRequest { worker_id, attempt, lease_token }
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["worker_id"], "worker-uuid-1234");
        assert_eq!(parsed["attempt"], 2);
        assert_eq!(parsed["lease_token"], "hex-lease-token");
        // Verify no extra fields leaked
        let keys: Vec<&str> = parsed.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        assert_eq!(keys.len(), 3);
    }

    // ── Cross-side: JudgeServer → API (ResultRequest) ──

    #[test]
    fn judgeserver_result_roundtrips_to_api() {
        let req = JudgeResultRequest {
            worker_id: "worker-uuid-5678".into(),
            attempt: 1,
            lease_token: "hex-lease-token".into(),
            result_id: "result-uuid-9abc".into(),
            outcome: "up".into(),
            exit_code: Some(0),
            stdout: Some("OK\n".into()),
            stderr: None,
            duration_ms: Some(123),
        };
        let json = serde_json::to_string(&req).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["worker_id"], "worker-uuid-5678");
        assert_eq!(parsed["attempt"], 1);
        assert_eq!(parsed["lease_token"], "hex-lease-token");
        assert_eq!(parsed["result_id"], "result-uuid-9abc");
        assert_eq!(parsed["outcome"], "up");
        assert_eq!(parsed["exit_code"], 0);
        assert_eq!(parsed["duration_ms"], 123);
    }

    // ── URL builders ──

    #[test]
    fn claim_url_builds_correctly() {
        assert_eq!(
            claim_url("http://api:9090", "evt-id"),
            "http://api:9090/internal/awd/events/evt-id/judge/claim"
        );
    }

    #[test]
    fn heartbeat_url_builds_correctly() {
        let task_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        assert_eq!(
            heartbeat_url("http://api:9090", "evt-id", &task_id),
            "http://api:9090/internal/awd/events/evt-id/judge/tasks/00000000-0000-0000-0000-000000000001/heartbeat"
        );
    }

    #[test]
    fn result_url_builds_correctly() {
        let task_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        assert_eq!(
            result_url("http://api:9090", "evt-id", &task_id),
            "http://api:9090/internal/awd/events/evt-id/judge/tasks/00000000-0000-0000-0000-000000000001/result"
        );
    }

    #[test]
    fn auth_header_has_bearer_prefix() {
        assert_eq!(auth_header("my-token"), "Bearer my-token");
    }
}