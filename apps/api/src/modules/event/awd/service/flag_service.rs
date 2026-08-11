//! Flag service — handles deterministic flag issuing for AWD events.

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::{
        AwdEventStatusExt, AwdPhaseExt, GameboxStatusExt,
        flag::{generate_flag, hash_flag},
    },
    repo::{ban_repo, event_repo, flag_repo, gamebox_repo, round_repo},
};

/// Context for issuing a flag: which GameBox, in which round, for which event.
pub struct FlagIssueContext {
    pub event_id: Uuid,
    pub round_id: Uuid,
    pub gamebox_instance_id: Uuid,
    pub source_ip: String,
}

/// Result of a flag issue operation.
pub struct FlagIssueResult {
    pub flag: String,
    pub already_issued: bool,
}

/// Issue a flag for a GameBox based on its source IP.
///
/// # Validation
/// - Event must be running in attack phase
/// - Active round must exist
/// - Team must not be banned
/// - Source IP must match a ready/running GameBox
/// - Flag is deterministic: same GameBox + same round = same flag
pub async fn issue_flag(
    db: &DatabaseConnection,
    ctx: FlagIssueContext,
    event_secret: &[u8],
    flag_prefix: &str,
) -> AwdResult<FlagIssueResult> {
    // 1. Verify event is running
    let awd_event = event_repo::find_by_event_id(db, ctx.event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_active() {
        return Err(AwdError::Forbidden(format!(
            "Event is not running (status: {:?})",
            awd_event.status
        )));
    }

    if !awd_event.phase.allows_flag_issue() {
        return Err(AwdError::Forbidden(format!(
            "Flag issuing not allowed in {:?} phase",
            awd_event.phase
        )));
    }

    // 2. Verify active round exists
    let round = round_repo::find_active_round(db, ctx.event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("No active round".into()))?;

    // 3. Verify GameBox exists by IP
    let instance = gamebox_repo::find_instance_by_ip(db, ctx.event_id, &ctx.source_ip)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound(format!("No GameBox found for IP: {}", ctx.source_ip)))?;

    if !instance.status.is_healthy() {
        return Err(AwdError::Forbidden(format!(
            "GameBox is not healthy (status: {:?})",
            instance.status
        )));
    }

    // 4. Check team is not banned
    let ban = ban_repo::find_active_ban(db, ctx.event_id, instance.team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if ban.is_some() {
        return Err(AwdError::Forbidden("Team is banned".into()));
    }

    // 5. Generate deterministic flag
    let flag = generate_flag(
        event_secret,
        &ctx.event_id.to_string(),
        &round.id.to_string(),
        &instance.id.to_string(),
        flag_prefix,
    );
    let flag_hash_str = hash_flag(&flag);

    // 6. Create or retrieve the flag issue record (idempotent)
    let existing = flag_repo::find_issue_by_hash(db, ctx.event_id, round.id, &flag_hash_str)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if let Some(issue) = existing {
        // Already issued — confirm the flag matches
        let recheck = generate_flag(
            event_secret,
            &ctx.event_id.to_string(),
            &round.id.to_string(),
            &instance.id.to_string(),
            flag_prefix,
        );
        if hash_flag(&recheck) == issue.flag_hash {
            return Ok(FlagIssueResult {
                flag: recheck,
                already_issued: true,
            });
        }
    }

    let _issue =
        flag_repo::find_or_create_issue(db, ctx.event_id, round.id, instance.id, &flag_hash_str)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(FlagIssueResult {
        flag,
        already_issued: false,
    })
}

/// Validate that a flag submission context is valid.
/// Returns (attacker_team_id, victim_team_id, gamebox_instance_id, flag_issue).
pub async fn validate_submission(
    db: &DatabaseConnection,
    event_id: Uuid,
    submitted_flag: &str,
    attacker_team_id: Uuid,
    attacker_user_id: Uuid,
) -> AwdResult<(
    Uuid, // flag_issue_id
    Uuid, // victim_team_id
    Uuid, // gamebox_instance_id
)> {
    // 1. Verify event is running and in attack phase
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_active() {
        return Err(AwdError::Forbidden("Event is not running".into()));
    }

    if !awd_event.phase.allows_flag_submission() {
        return Err(AwdError::Forbidden(
            "Flag submission not allowed in current phase".into(),
        ));
    }

    // 2. Verify active round
    let round = round_repo::find_active_round(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("No active round".into()))?;

    // 3. Find flag by hash
    let flag_hash_str = hash_flag(submitted_flag);
    let issue = flag_repo::find_issue_by_hash(db, event_id, round.id, &flag_hash_str)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("Invalid or expired flag".into()))?;

    // 4. Get GameBox instance to find victim team
    let instance = gamebox_repo::find_instance_by_id(db, issue.gamebox_instance_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox instance not found".into()))?;

    let victim_team_id = instance.team_id;

    // 5. Reject self-attack
    if victim_team_id == attacker_team_id {
        return Err(AwdError::Forbidden(
            "Cannot submit your own team's flag".into(),
        ));
    }

    // 6. Check attacker not banned
    let ban = ban_repo::find_active_ban(db, event_id, attacker_team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if ban.is_some() {
        return Err(AwdError::Forbidden("Your team is banned".into()));
    }

    Ok((issue.id, victim_team_id, instance.id))
}
