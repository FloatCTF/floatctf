//! Jeopardy 动态分计算。

use sea_orm::DbConn;

use crate::infrastructure::settings::get_setting;

/// 由基础分与已解次数计算动态分（纯函数）。
/// `solves` = 本次加分**之前**已记录的解题次数（与历史语义一致）。
pub fn dynamic_score(base_points: f64, solves: u64, decay: f64, min_percent: f64) -> f64 {
    if solves == 0 {
        return base_points;
    }
    let min_points = base_points * min_percent;
    let current =
        min_points + (base_points - min_points) * ((decay / (decay + solves as f64)).sqrt());
    current.max(min_points)
}

/// 从设置读取衰减/最低百分比，再计算动态分。
pub async fn calculate_next_dynamic_score(
    db: &DbConn,
    base_points: f64,
    solves: u64,
) -> anyhow::Result<f64> {
    if solves == 0 {
        return Ok(base_points);
    }
    let decay = get_setting(db, "EVENT_SCORE_DECAY").await?.parse::<f64>()?;
    let min_percent = get_setting(db, "EVENT_SCORE_MIN_PERCENT")
        .await?
        .parse::<f64>()?;
    Ok(dynamic_score(base_points, solves, decay, min_percent))
}

#[cfg(test)]
mod tests {
    use super::dynamic_score;

    #[test]
    fn zero_solves_returns_base() {
        assert_eq!(dynamic_score(500.0, 0, 15.0, 0.45), 500.0);
    }

    #[test]
    fn decays_with_solves() {
        let s0 = dynamic_score(500.0, 0, 15.0, 0.45);
        let s10 = dynamic_score(500.0, 10, 15.0, 0.45);
        assert!(s10 < s0);
        assert!(s10 >= 500.0 * 0.45);
    }
}
