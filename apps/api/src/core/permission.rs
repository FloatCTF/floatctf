//! 平台权限目录 —— **代码是唯一权威**。
//!
//! # 为什么权限是代码概念而不是数据概念
//!
//! 每一项权限都必须由代码去 `require`（否则没有执行者）。若把权限做成 DB catalog 表，
//! 代码侧仍然需要一个 enum，于是产生**两个真相源**：DB 里的行与代码里的变体可能漂移，
//! 而且每加一个权限都要写一条 migration。
//!
//! 因此本模块的 [`Permission`] enum 是权威；`platform_role_permissions.permission`
//! 只存**稳定字符串**，写路径必须经 [`Permission::from_str`] 校验。
//!
//! # grants_all
//!
//! `super-admin` 用 `platform_roles.grants_all = true` 表达全权，而不是一堆 permission 行。
//! 原因：若是行集合，删一行或将来漏补一行就会让最后一个管理员**静默失去**能力。
//! `grants_all` 让「能做一切，包括明天才新增的权限」成为结构性保证（见 [`Permission::ALL`]）。
//!
//! # 命名冲突警告
//!
//! 本模块的 `permission` **不是** `PLATFORM_CAPABILITIES`（`GET /api/frontend` 的静态
//! 平台能力）也**不是** `EventCapabilities`（赛制能力位）。三者语义完全不同，禁止互相复用。

use crate::api::extractor::auth::RequirePermission;
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 权限所属域：仅用于 UI 分组展示与文档，**不参与授权判定**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PermissionDomain {
    Identity,
    Content,
    Events,
    Awd,
    Awdp,
    Community,
    Platform,
    Scheduler,
    Infra,
    Terminal,
}

impl PermissionDomain {
    /// 全部域（顺序即 UI 展示顺序）。
    pub const ALL: &'static [PermissionDomain] = &[
        PermissionDomain::Identity,
        PermissionDomain::Content,
        PermissionDomain::Events,
        PermissionDomain::Awd,
        PermissionDomain::Awdp,
        PermissionDomain::Community,
        PermissionDomain::Platform,
        PermissionDomain::Scheduler,
        PermissionDomain::Infra,
        PermissionDomain::Terminal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PermissionDomain::Identity => "identity",
            PermissionDomain::Content => "content",
            PermissionDomain::Events => "events",
            PermissionDomain::Awd => "awd",
            PermissionDomain::Awdp => "awdp",
            PermissionDomain::Community => "community",
            PermissionDomain::Platform => "platform",
            PermissionDomain::Scheduler => "scheduler",
            PermissionDomain::Infra => "infra",
            PermissionDomain::Terminal => "terminal",
        }
    }
}

impl fmt::Display for PermissionDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for PermissionDomain {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 无法解析为已知权限的字符串。
///
/// 出现它意味着：DB 里有代码不认识的权限字符串（升级回退 / 手工改库 / 权限被重命名）。
/// 调用方必须**显式处理**（WARN 或拒绝写入），不得静默忽略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownPermission(pub String);

impl fmt::Display for UnknownPermission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown permission: {}", self.0)
    }
}

impl std::error::Error for UnknownPermission {}

/// 声明单条权限。字段顺序：
/// `变体 => "稳定字符串", 域, 是否高危, "说明", 守卫标记类型`
macro_rules! define_permissions {
    ($(
        $variant:ident => $key:literal, $domain:ident, $risk:expr, $desc:literal, $guard:ident;
    )+) => {
        /// 一条平台权限。**这是权限的唯一权威定义。**
        ///
        /// 序列化为其稳定字符串（如 `"events.manage"`）；反序列化时未知字符串会
        /// **失败**而不是落到默认值。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Permission {
            $(
                #[doc = $desc]
                $variant,
            )+
        }

        impl Permission {
            /// 全部权限（顺序稳定，与声明顺序一致）。
            ///
            /// `grants_all` 的角色短路返回本列表，因此**新增权限会自动被 super-admin 拥有**——
            /// 这正是选择 `grants_all` 而非 permission 行集合的原因。
            pub const ALL: &'static [Permission] = &[
                $( Permission::$variant, )+
            ];

            /// 稳定字符串（落库 / API 输出 / 审计都用它）。
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Permission::$variant => $key, )+
                }
            }

            /// 所属域（仅展示用）。
            pub const fn domain(self) -> PermissionDomain {
                match self {
                    $( Permission::$variant => PermissionDomain::$domain, )+
                }
            }

            /// 人类可读说明。
            pub const fn description(self) -> &'static str {
                match self {
                    $( Permission::$variant => $desc, )+
                }
            }

            /// 是否高危权限（UI danger 标注 + 二次确认 + 审计重点）。
            pub const fn high_risk(self) -> bool {
                match self {
                    $( Permission::$variant => $risk, )+
                }
            }
        }

        impl fmt::Display for Permission {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for Permission {
            type Err = UnknownPermission;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $( $key => Ok(Permission::$variant), )+
                    other => Err(UnknownPermission(other.to_string())),
                }
            }
        }

        impl Serialize for Permission {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for Permission {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                Permission::from_str(&raw).map_err(serde::de::Error::custom)
            }
        }

        /// 声明式权限守卫的**标记类型**。
        ///
        /// 与 [`Permission`] 由**同一个宏**生成，因此「有权限却没有守卫类型」在结构上
        /// 不可能发生。Wave 5 的 `RequirePermission<P>` 以这些零大小类型为类型参数，
        /// 把「handler 需要哪条权限」写进函数签名——忘记声明权限就编译不过，
        /// 而不是变成「登录即可用」的静默漏洞。
        pub mod guards {
            use super::{Permission, PermissionMarker};

            $(
                #[doc = concat!("`", $key, "` 的声明式守卫标记。")]
                #[derive(Debug, Clone, Copy)]
                pub struct $guard;

                impl PermissionMarker for $guard {
                    const PERMISSION: Permission = Permission::$variant;
                }
            )+
        }
    };
}

define_permissions! {
    // ── Identity ────────────────────────────────────────────────────────────
    UsersRead => "users.read", Identity, false,
        "查看账号列表、详情与其角色", UsersReadGuard;
    UsersManage => "users.manage", Identity, false,
        "创建 / 修改 / 删除 / 启用禁用账号", UsersManageGuard;
    RolesRead => "roles.read", Identity, false,
        "查看角色目录与权限目录", RolesReadGuard;
    RolesManage => "roles.manage", Identity, true,
        "增删改角色，以及给账号授予/撤销角色", RolesManageGuard;

    // ── Content ─────────────────────────────────────────────────────────────
    ChallengesRead => "challenges.read", Content, false,
        "查看题目与题集", ChallengesReadGuard;
    ChallengesManage => "challenges.manage", Content, false,
        "题目/题集增删改，以及导入/构建/扫描/检查", ChallengesManageGuard;
    GameboxesRead => "gameboxes.read", Content, false,
        "查看 GameBox 库", GameboxesReadGuard;
    GameboxesManage => "gameboxes.manage", Content, false,
        "GameBox 导入/构建/扫描/检查/修改/隐藏/删除", GameboxesManageGuard;
    WeaponsRead => "weapons.read", Content, false,
        "查看武器库", WeaponsReadGuard;
    WeaponsManage => "weapons.manage", Content, false,
        "武器库增删改与文件上传", WeaponsManageGuard;

    // ── Events ──────────────────────────────────────────────────────────────
    EventsRead => "events.read", Events, false,
        "查看赛事、数据、报表、参与记录、日志、Writeup 与私有制品下载", EventsReadGuard;
    EventsManage => "events.manage", Events, false,
        "赛事增删改、赛题挂载、赛事公告", EventsManageGuard;
    EventsParticipantsManage => "events.participants.manage", Events, false,
        "赛事的参赛用户/队伍增删与赛事级封禁", EventsParticipantsManageGuard;
    EventsLifecycleManage => "events.lifecycle.manage", Events, false,
        "推动比赛：开始/暂停/恢复/结束/归档/部署/预检/重置靶机/调分/轮换令牌/网络生效/AWD 封队",
        EventsLifecycleManageGuard;

    // ── AWD ─────────────────────────────────────────────────────────────────
    AwdRead => "awd.read", Awd, false,
        "查看 AWD 配置、预检、判题批次、分数与网络", AwdReadGuard;
    AwdManage => "awd.manage", Awd, false,
        "配置 AWD 赛事与赛事 GameBox 挂载", AwdManageGuard;

    // ── AWDP ────────────────────────────────────────────────────────────────
    AwdpRead => "awdp.read", Awdp, false,
        "查看 AWDP 配置、run、实例、分数与数据", AwdpReadGuard;
    AwdpManage => "awdp.manage", Awdp, false,
        "配置 AWDP 赛事与 GameBox 挂载", AwdpManageGuard;

    // ── Community ───────────────────────────────────────────────────────────
    AnnouncementsManage => "announcements.manage", Community, false,
        "平台公告增删改查", AnnouncementsManageGuard;
    DiscussionsModerate => "discussions.moderate", Community, false,
        "讨论与评论的审核删除", DiscussionsModerateGuard;

    // ── Platform ────────────────────────────────────────────────────────────
    SettingsRead => "settings.read", Platform, false,
        "查看平台设置", SettingsReadGuard;
    SettingsManage => "settings.manage", Platform, true,
        "增删改平台设置（含 FRONTEND_ACTIVE）", SettingsManageGuard;
    SystemRead => "system.read", Platform, false,
        "仪表盘、系统监控与版本信息", SystemReadGuard;
    LogsRead => "logs.read", Platform, false,
        "查看平台与赛事审计日志", LogsReadGuard;

    // ── Scheduler ───────────────────────────────────────────────────────────
    SchedulerManage => "scheduler.manage", Scheduler, false,
        "计划任务增删改与手动运行", SchedulerManageGuard;

    // ── Infra ───────────────────────────────────────────────────────────────
    DockerRead => "docker.read", Infra, false,
        "查看容器、镜像与网络", DockerReadGuard;
    DockerManage => "docker.manage", Infra, true,
        "容器启停删、镜像删除、网络增删", DockerManageGuard;
    DatabaseExecute => "database.execute", Infra, true,
        "执行任意 SQL", DatabaseExecuteGuard;

    // ── Terminal ────────────────────────────────────────────────────────────
    TerminalOpen => "terminal.open", Terminal, true,
        "打开宿主机 Web 终端", TerminalOpenGuard;
}

impl Permission {
    /// 解析稳定字符串；未知返回 `None`。
    ///
    /// 与 [`FromStr`] 的区别：用于「存在性判断」场景（如 orphan 权限扫描），
    /// 不需要构造错误值。
    pub fn parse(raw: &str) -> Option<Permission> {
        Permission::from_str(raw).ok()
    }
}

/// 声明式守卫的标记类型契约。
///
/// Wave 5 的 `RequirePermission<P: PermissionMarker>` 用它把「handler 需要哪条权限」
/// 提升为**类型**，从而让「忘记做权限检查」变成编译错误。
pub trait PermissionMarker {
    const PERMISSION: Permission;
}

/// 规范化权限集合：多角色取并集，无 deny 语义、无优先级。
///
/// 用 `BTreeSet` 而非 `HashSet`：`to_sorted_strings()` 的输出必须**确定性**
/// （`/me` 的 permissions 数组直接喂给前端 `Set`，稳定顺序便于测试与 diff）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionSet(BTreeSet<Permission>);

impl PermissionSet {
    /// 空集合。
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// 全部权限的集合（`grants_all` 角色的短路结果）。
    pub fn all() -> Self {
        Self(Permission::ALL.iter().copied().collect())
    }

    pub fn from_permissions<I: IntoIterator<Item = Permission>>(items: I) -> Self {
        Self(items.into_iter().collect())
    }

    pub fn contains(&self, permission: Permission) -> bool {
        self.0.contains(&permission)
    }

    pub fn insert(&mut self, permission: Permission) -> bool {
        self.0.insert(permission)
    }

    /// 并入另一个集合（多角色并集）。
    pub fn extend(&mut self, other: &PermissionSet) {
        self.0.extend(other.0.iter().copied());
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Permission> + '_ {
        self.0.iter().copied()
    }

    /// `self ⊆ other`：用于反提权规则（actor 只能操作自己权限范围内的对象）。
    pub fn is_subset_of(&self, other: &PermissionSet) -> bool {
        self.0.is_subset(&other.0)
    }

    /// 解析一组来自 DB / 请求的权限字符串。
    ///
    /// 返回 `(已识别集合, 未识别字符串)`。**未识别的字符串不会让整个调用失败**，
    /// 但必须由调用方显式处理：
    ///
    /// - **启动自检**：WARN 并列出（不静默删除——删除权限是安全相关动作）；
    /// - **写路径**：直接拒绝（`AppError::Validation`），不允许写入未知权限。
    pub fn from_strings<I, S>(items: I) -> (Self, Vec<String>)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = BTreeSet::new();
        let mut unknown = Vec::new();
        for item in items {
            let raw = item.as_ref();
            match Permission::from_str(raw) {
                Ok(permission) => {
                    set.insert(permission);
                }
                Err(_) => unknown.push(raw.to_string()),
            }
        }
        (Self(set), unknown)
    }

    /// 稳定字符串列表（按 `Permission` 的声明顺序，保证确定性）。
    pub fn to_strings(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect()
    }
}

impl FromIterator<Permission> for PermissionSet {
    fn from_iter<T: IntoIterator<Item = Permission>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for PermissionSet {
    type Item = Permission;
    type IntoIter = std::collections::btree_set::IntoIter<Permission>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录规模基线：Wave 1 的 29 条。新增/删除权限必须显式更新本测试。
    const EXPECTED_PERMISSION_COUNT: usize = 29;

    #[test]
    fn permission_catalog_size_is_pinned() {
        assert_eq!(
            Permission::ALL.len(),
            EXPECTED_PERMISSION_COUNT,
            "权限目录规模变化时必须同步 platform-permissions.md / SDK 类型 / 本测试"
        );
    }

    #[test]
    fn permission_all_has_no_duplicates() {
        let mut seen = BTreeSet::new();
        for permission in Permission::ALL {
            assert!(
                seen.insert(permission.as_str()),
                "duplicate permission string: {}",
                permission.as_str()
            );
        }
    }

    /// `ALL` 必须覆盖**每一个**变体。用穷尽 match 让「新增变体但忘了加入 ALL」
    /// 变成编译错误（而不是运行期静默漏权限）。
    #[test]
    fn permission_all_covers_every_variant() {
        for permission in Permission::ALL {
            let matched = match permission {
                Permission::UsersRead
                | Permission::UsersManage
                | Permission::RolesRead
                | Permission::RolesManage
                | Permission::ChallengesRead
                | Permission::ChallengesManage
                | Permission::GameboxesRead
                | Permission::GameboxesManage
                | Permission::WeaponsRead
                | Permission::WeaponsManage
                | Permission::EventsRead
                | Permission::EventsManage
                | Permission::EventsParticipantsManage
                | Permission::EventsLifecycleManage
                | Permission::AwdRead
                | Permission::AwdManage
                | Permission::AwdpRead
                | Permission::AwdpManage
                | Permission::AnnouncementsManage
                | Permission::DiscussionsModerate
                | Permission::SettingsRead
                | Permission::SettingsManage
                | Permission::SystemRead
                | Permission::LogsRead
                | Permission::SchedulerManage
                | Permission::DockerRead
                | Permission::DockerManage
                | Permission::DatabaseExecute
                | Permission::TerminalOpen => true,
            };
            assert!(matched);
        }
    }

    #[test]
    fn permission_strings_round_trip() {
        for permission in Permission::ALL {
            let raw = permission.as_str();
            let parsed = Permission::from_str(raw).expect("known permission must parse");
            assert_eq!(parsed, *permission);
            assert_eq!(parsed.to_string(), raw);
        }
    }

    #[test]
    fn permission_strings_match_database_format_constraint() {
        // 与 migration 的 CHECK (permission ~ '^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$') 对齐
        for permission in Permission::ALL {
            let raw = permission.as_str();
            assert!(
                raw.split('.').count() >= 2,
                "permission must have at least two segments: {raw}"
            );
            for segment in raw.split('.') {
                assert!(!segment.is_empty(), "empty segment in {raw}");
                let mut chars = segment.chars();
                let first = chars.next().expect("non-empty segment");
                assert!(first.is_ascii_lowercase(), "segment must start a-z: {raw}");
                assert!(
                    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                    "segment must be [a-z0-9]: {raw}"
                );
            }
        }
    }

    #[test]
    fn permission_parsing_rejects_invalid_input() {
        for raw in [
            "",
            "USERS.READ",
            "users",
            "users.",
            ".users",
            "users..read",
            "users. read",
            "users-read",
            "unknown.permission",
        ] {
            assert!(
                Permission::from_str(raw).is_err(),
                "{raw:?} must not parse as a permission"
            );
        }
    }

    #[test]
    fn permission_deserialization_fails_on_unknown_instead_of_defaulting() {
        // 关键：未知权限**不得**回落成某个默认权限（那会静默放行或静默拒绝）
        let err = serde_json::from_str::<Permission>("\"not.a.permission\"");
        assert!(err.is_err(), "unknown permission must fail to deserialize");

        let ok: Permission = serde_json::from_str("\"events.manage\"").unwrap();
        assert_eq!(ok, Permission::EventsManage);
    }

    #[test]
    fn high_risk_permissions_are_exactly_the_five_reviewed_ones() {
        let high_risk: Vec<&str> = Permission::ALL
            .iter()
            .filter(|p| p.high_risk())
            .map(|p| p.as_str())
            .collect();
        assert_eq!(
            high_risk,
            vec![
                "roles.manage",
                "settings.manage",
                "docker.manage",
                "database.execute",
                "terminal.open",
            ]
        );
    }

    #[test]
    fn every_domain_has_at_least_one_permission() {
        for domain in PermissionDomain::ALL {
            let count = Permission::ALL
                .iter()
                .filter(|p| p.domain() == *domain)
                .count();
            assert!(count > 0, "domain {domain} has no permission");
        }
    }

    #[test]
    fn guards_expose_matching_permission() {
        use guards::*;
        assert_eq!(UsersReadGuard::PERMISSION, Permission::UsersRead);
        assert_eq!(EventsManageGuard::PERMISSION, Permission::EventsManage);
        assert_eq!(TerminalOpenGuard::PERMISSION, Permission::TerminalOpen);
        // 守卫类型必须与 enum 数量一一对应
        let guard_permissions = [
            UsersReadGuard::PERMISSION,
            UsersManageGuard::PERMISSION,
            RolesReadGuard::PERMISSION,
            RolesManageGuard::PERMISSION,
            ChallengesReadGuard::PERMISSION,
            ChallengesManageGuard::PERMISSION,
            GameboxesReadGuard::PERMISSION,
            GameboxesManageGuard::PERMISSION,
            WeaponsReadGuard::PERMISSION,
            WeaponsManageGuard::PERMISSION,
            EventsReadGuard::PERMISSION,
            EventsManageGuard::PERMISSION,
            EventsParticipantsManageGuard::PERMISSION,
            EventsLifecycleManageGuard::PERMISSION,
            AwdReadGuard::PERMISSION,
            AwdManageGuard::PERMISSION,
            AwdpReadGuard::PERMISSION,
            AwdpManageGuard::PERMISSION,
            AnnouncementsManageGuard::PERMISSION,
            DiscussionsModerateGuard::PERMISSION,
            SettingsReadGuard::PERMISSION,
            SettingsManageGuard::PERMISSION,
            SystemReadGuard::PERMISSION,
            LogsReadGuard::PERMISSION,
            SchedulerManageGuard::PERMISSION,
            DockerReadGuard::PERMISSION,
            DockerManageGuard::PERMISSION,
            DatabaseExecuteGuard::PERMISSION,
            TerminalOpenGuard::PERMISSION,
        ];
        assert_eq!(guard_permissions.len(), Permission::ALL.len());
        for (index, permission) in guard_permissions.iter().enumerate() {
            assert_eq!(*permission, Permission::ALL[index]);
        }
    }

    // ── PermissionSet ───────────────────────────────────────────────────────

    #[test]
    fn permission_set_all_contains_every_permission() {
        let all = PermissionSet::all();
        assert_eq!(all.len(), EXPECTED_PERMISSION_COUNT);
        for permission in Permission::ALL {
            assert!(all.contains(*permission));
        }
    }

    #[test]
    fn permission_set_union_is_idempotent_and_commutative() {
        let a = PermissionSet::from_permissions([Permission::EventsRead, Permission::AwdRead]);
        let b = PermissionSet::from_permissions([Permission::AwdRead, Permission::AwdpRead]);

        let mut ab = a.clone();
        ab.extend(&b);
        let mut ba = b.clone();
        ba.extend(&a);
        assert_eq!(ab, ba, "并集必须可交换");
        assert_eq!(ab.len(), 3, "重复权限只应出现一次");

        let mut a_twice = a.clone();
        a_twice.extend(&a);
        assert_eq!(a_twice, a, "并集必须幂等");
    }

    #[test]
    fn permission_set_subset_is_the_anti_escalation_primitive() {
        let actor = PermissionSet::from_permissions([Permission::EventsRead, Permission::AwdRead]);
        let within = PermissionSet::from_permissions([Permission::EventsRead]);
        let beyond =
            PermissionSet::from_permissions([Permission::EventsRead, Permission::TerminalOpen]);

        assert!(within.is_subset_of(&actor));
        assert!(!beyond.is_subset_of(&actor));
        // 空集是任何集合的子集（「不授予任何权限」永远合法）
        assert!(PermissionSet::new().is_subset_of(&actor));
        // 全集不是有限 actor 权限的子集
        assert!(!PermissionSet::all().is_subset_of(&actor));
    }

    #[test]
    fn permission_set_from_strings_reports_unknown_without_failing() {
        let (set, unknown) = PermissionSet::from_strings([
            "events.read",
            "users.manage",
            "events.read",
            "totally.unknown",
            "legacy.permission",
        ]);
        assert_eq!(set.len(), 2, "重复项应去重");
        assert!(set.contains(Permission::EventsRead));
        assert!(set.contains(Permission::UsersManage));
        assert_eq!(unknown, vec!["totally.unknown", "legacy.permission"]);
    }

    #[test]
    fn permission_set_to_strings_is_deterministic() {
        let a = PermissionSet::from_strings(["users.manage", "events.read", "terminal.open"]).0;
        let b = PermissionSet::from_strings(["terminal.open", "events.read", "users.manage"]).0;
        assert_eq!(a.to_strings(), b.to_strings(), "输出顺序必须与输入顺序无关");
        // 与声明顺序一致
        assert_eq!(
            a.to_strings(),
            vec!["users.manage", "events.read", "terminal.open"]
        );
    }
}
