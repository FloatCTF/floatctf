export enum AwdEventStatus {
  Draft = 'draft',
  Configuring = 'configuring',
  Deploying = 'deploying',
  Deployed = 'deployed',
  Prechecking = 'prechecking',
  Verified = 'verified',
  Running = 'running',
  Paused = 'paused',
  NetworkError = 'network_error',
  StartBlocked = 'start_blocked',
  Finished = 'finished',
  Archived = 'archived',
  DeployFailed = 'deploy_failed',
  VerificationFailed = 'verification_failed',
}

export enum AwdNetworkAllocationKind {
  Gamebox = 'gamebox',
  Wireguard = 'wireguard',
}

export enum AwdNetworkAllocationMode {
  Automatic = 'automatic',
  Manual = 'manual',
}

export enum AwdPhase {
  Hardening = 'hardening',
  Attack = 'attack',
  Pause = 'pause',
}

export enum AwdpEvaluationKind {
  Manual = 'manual',
  Official = 'official',
}

export enum AwdpEvaluationStatus {
  Pending = 'pending',
  Running = 'running',
  NoPatch = 'no_patch',
  ServiceDown = 'service_down',
  FunctionalBroken = 'functional_broken',
  Vulnerable = 'vulnerable',
  Patched = 'patched',
  PlatformError = 'platform_error',
}

export enum AwdpPhase {
  Pending = 'pending',
  Break = 'break',
  Fix = 'fix',
  Ended = 'ended',
}

export enum BanStatus {
  Active = 'active',
  PendingUnban = 'pending_unban',
  Unbanned = 'unbanned',
}

export enum EventFamily {
  Jeopardy = 'jeopardy',
  Awd = 'awd',
  Awdp = 'awdp',
}

export enum EventPurpose {
  Practice = 'practice',
  Competition = 'competition',
}

export enum EventTeamMemberRole {
  Captain = 'captain',
  Member = 'member',
}

export enum GameboxStatus {
  Pending = 'pending',
  Creating = 'creating',
  Running = 'running',
  Ready = 'ready',
  Resetting = 'resetting',
  Missing = 'missing',
  Orphan = 'orphan',
  Conflict = 'conflict',
  StartFailed = 'start_failed',
  ResetFailed = 'reset_failed',
  Stopped = 'stopped',
}

export enum JudgeTaskStatus {
  Pending = 'pending',
  Running = 'running',
  Up = 'up',
  Down = 'down',
  JudgeError = 'judge_error',
  JudgeTimeout = 'judge_timeout',
  SkippedResetting = 'skipped_resetting',
  SkippedBanned = 'skipped_banned',
}

export enum ParticipantMode {
  Individual = 'individual',
  Team = 'team',
}

export enum PrecheckStatus {
  Pending = 'pending',
  Running = 'running',
  Passed = 'passed',
  Failed = 'failed',
  Error = 'error',
}

export enum RoundStatus {
  Active = 'active',
  Grace = 'grace',
  Completed = 'completed',
  Paused = 'paused',
}

export enum ScoreEventType {
  Attack = 'attack',
  VictimLoss = 'victim_loss',
  JudgeFix = 'judge_fix',
  JudgeDown = 'judge_down',
  FirstBonus = 'first_bonus',
  ResetPenalty = 'reset_penalty',
  Adjustment = 'adjustment',
}

export enum SettingValueType {
  String = 'string',
  Integer = 'integer',
  Boolean = 'boolean',
  Float = 'float',
}

export enum WgPeerStatus {
  Active = 'active',
  Revoked = 'revoked',
  Rotating = 'rotating',
}