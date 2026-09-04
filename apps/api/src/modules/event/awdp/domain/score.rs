//! AWDP 计分幂等键（与 awd_score_events.idempotency_key 模式一致，语义独立）。

use uuid::Uuid;

/// 双主体键：`user:{id}` 或 `team:{id}`。
pub fn subject_key(user_id: Option<Uuid>, team_id: Option<Uuid>) -> String {
    match (user_id, team_id) {
        (Some(u), None) => format!("user:{u}"),
        (None, Some(t)) => format!("team:{t}"),
        _ => unreachable!("exactly-one owner"),
    }
}

/// Break 一次性：`awdp:break:{run}:{gamebox}:{subject}`。
pub fn break_idempotency_key(
    run_id: Uuid,
    gamebox_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
) -> String {
    format!(
        "awdp:break:{run_id}:{gamebox_id}:{}",
        subject_key(user_id, team_id)
    )
}

/// Fix 每轮：`awdp:fix:{run}:{fix_round}:{instance}`。
pub fn fix_idempotency_key(run_id: Uuid, fix_round_id: Uuid, instance_id: Uuid) -> String {
    format!("awdp:fix:{run_id}:{fix_round_id}:{instance_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn keys_are_stable_and_unique() {
        let r = Uuid::from_str("00000000-0000-0000-0000-000000000001").unwrap();
        let g = Uuid::from_str("00000000-0000-0000-0000-000000000002").unwrap();
        let rnd = Uuid::from_str("00000000-0000-0000-0000-000000000003").unwrap();
        let i = Uuid::from_str("00000000-0000-0000-0000-000000000004").unwrap();
        let u = Some(Uuid::from_str("00000000-0000-0000-0000-000000000005").unwrap());
        let t = Some(Uuid::from_str("00000000-0000-0000-0000-000000000006").unwrap());

        let k1 = break_idempotency_key(r, g, u, None);
        let k2 = break_idempotency_key(r, g, None, t);
        assert_ne!(k1, k2);
        assert!(k1.starts_with("awdp:break:"));
        assert_eq!(break_idempotency_key(r, g, u, None), k1, "稳定");

        let f1 = fix_idempotency_key(r, rnd, i);
        assert!(f1.starts_with("awdp:fix:"));
        assert_eq!(fix_idempotency_key(r, rnd, i), f1, "稳定");
    }
}
