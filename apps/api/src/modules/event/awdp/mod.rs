//! AWD Plus（`EventFamily::Awdp`）模块 —— **骨架占位**。
//!
//! # 当前状态
//!
//! 本模块仅为 AWD Plus 赛制族预留命名空间，**未实现任何引擎逻辑**：
//!
//! - 无 awdp 专属表结构（复用 `events` 身份模型，`EventMode::awdp_team_competition()`）
//! - 无 awdp 选手端 / 管理端 / 内部路由
//! - 无调度任务、无 GameBox / FlagServer / JudgeServer 集成
//!
//! # 能力位
//!
//! 骨架期 `EventCapabilities::for_mode` 对 `Awdp` 复用攻防能力位
//! （`supports_gameboxes / supports_rounds / supports_judge / …` 为 true），
//! 待正式引擎落地时按 AWD Plus 真实能力拆分支。
//!
//! # 演进方向（未实现）
//!
//! AWD Plus = 攻防 + 修复（Attack-Defense-Plus）：在 AWD 基础上引入
//! 题目补丁 / 修复回合等机制，表结构与服务形态待设计。
