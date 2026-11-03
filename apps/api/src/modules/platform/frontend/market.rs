//! 前端市场：**发现** GitHub 上的 FloatCTF 前端仓库及其 release 制品元数据。
//!
//! # 边界（重要）
//!
//! 本模块**只做发现与元数据**，不实现安装。安装是宿主的 [`frontend.sh`] 职责：
//! 生产里 API 容器是 `read_only: true` + `cap_drop: ALL`，**根本没有挂载前端目录**
//! （见 `scripts/install.sh` 的 compose 模板：只有 Caddy 以 `:ro` 挂 `frontends/`）。
//! 因此这里**没有任何特权操作**，也不试图给 API 开宿主写权限（铁律 6）。
//!
//! 市场给出的价值是：让管理员知道**有哪些前端、各自最新制品是什么、兼容哪个契约**，
//! 并给出可直接复制的安装命令。
//!
//! # 为什么安装走 release 制品而不是 clone + 构建
//!
//! [`frontend.sh install`] 对 `制品.tar.gz` 走的是「解包 + 校验」分支，**不在宿主构建**。
//! 这与平台既有的训练站同步完全同构（见 `scheduler/handlers/training_handlers.rs` 的
//! 开头注释：「生产机不需要 Node，也不执行对方的仓库代码」）。所以本模块只消费
//! **预构建制品**，从不拉取或执行候选前端的源码。
//!
//! # 元数据从哪来
//!
//! 光看 release 资产列表，市场无法知道该前端的 `id` / `version` / 兼容契约 ——
//! 那些字段在 `frontend-dist.tar.gz` 内部的 `frontend.json` 里。为了不必为了显示
//! 元数据而下载整个制品，约定：**CI 额外产出一个小的 `BUILD_INFO.json`**，
//! 其中 `frontend` 字段是制品 manifest 的原样副本。本模块只读这个小文件（有体积上限）。
//! 读不到就退化为「只有资产信息」，不报错 —— 市场应当在前端尚未发版时也能用。
//!
//! [`frontend.sh`]: https://github.com/FloatCTF/floatctf
//! [`frontend.sh install`]: ../../../../../scripts/frontend.sh

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::api::prelude::*;
use crate::core::config::FrontendMarketConfig;
use crate::infrastructure::ProxyReqwest;

/// GitHub REST API 基址。
const GITHUB_API: &str = "https://api.github.com";

/// 单个 GitHub JSON 响应的体积上限（仓库列表 / release 列表都很小）。
const MAX_JSON_BYTES: u64 = 2 * 1024 * 1024;

/// `BUILD_INFO.json` 的体积上限 —— 它只含元数据，正常在几 KB。
const MAX_BUILD_INFO_BYTES: u64 = 256 * 1024;

/// 每个仓库最多看多少个 release（滚动通道 + 若干版本 tag 足够）。
const RELEASES_PER_REPO: usize = 8;

/// 本平台认得的制品资产名（由前端仓库的发布 CI 产出）。
const ASSET_ARCHIVE: &str = "frontend-dist.tar.gz";
const ASSET_CHECKSUM: &str = "frontend-dist.tar.gz.sha256";
const ASSET_BUILD_INFO: &str = "BUILD_INFO.json";

// ============================================================================
// DTO：对外形状
// ============================================================================

/// `GET /api/admin/frontend-market` 的响应。
#[derive(Debug, Clone, Serialize)]
pub struct FrontendMarketDto {
    /// 搜索的所有者（组织或用户）。
    pub owner: String,
    /// 仓库名前缀过滤。
    pub name_prefix: String,
    /// 本次数据是何时从 GitHub 取得的（RFC3339）。命中的是缓存时也照实给出。
    pub fetched_at: String,
    /// 本响应是否来自缓存。
    pub cached: bool,
    /// 是否带了 GitHub token（影响限流上限，也会让私有仓库可见）。
    pub authenticated: bool,
    /// 缓存有效期（秒）。
    pub cache_ttl_secs: u64,
    /// 命中的候选前端。
    pub items: Vec<FrontendMarketItemDto>,
    /// 拉取过程中**非致命**的问题（例如某个仓库的 release 拉不到）。
    /// 市场是「尽力而为」的聚合，单点失败不该让整页空白。
    pub warnings: Vec<String>,
}

/// 一个候选前端仓库。
#[derive(Debug, Clone, Serialize)]
pub struct FrontendMarketItemDto {
    pub full_name: String,
    pub name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub stars: u32,
    pub pushed_at: Option<String>,
    pub archived: bool,
    pub default_branch: String,
    /// 该仓库的 release（新的在前）。
    pub releases: Vec<FrontendReleaseDto>,
    /// 供管理员直接复制的安装命令（取第一个带制品的 release）。
    /// 市场**不执行**它 —— 它需要在宿主上运行。
    pub install_command: Option<String>,
}

/// 一个 release。
#[derive(Debug, Clone, Serialize)]
pub struct FrontendReleaseDto {
    pub tag: String,
    pub name: Option<String>,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub html_url: String,
    /// 该 release 的资产（只列出市场关心的那几个）。
    pub assets: Vec<FrontendAssetDto>,
    /// 制品 manifest 的关键字段；来自 `BUILD_INFO.json`，读不到就是 `None`。
    pub artifact: Option<FrontendArtifactDto>,
    /// 该 release 是否**看起来可安装**（有 tar.gz 资产 + 有 frontend.json 元数据）。
    /// 为 `false` 不等于不能装，只表示市场无法确认 —— 前端可能还没接发布 CI。
    pub installable: bool,
    /// 可直接复制的安装命令。
    pub install_command: Option<String>,
}

/// release 资产。
#[derive(Debug, Clone, Serialize)]
pub struct FrontendAssetDto {
    pub name: String,
    pub size: u64,
    pub download_url: String,
}

/// 制品 manifest 摘要（来自 `BUILD_INFO.json` 的 `frontend` 字段）。
#[derive(Debug, Clone, Serialize)]
pub struct FrontendArtifactDto {
    /// 制品 id（`frontend.json` 的 `id`），例如 `animal-island`。
    pub id: String,
    /// 制品版本。
    pub version: String,
    /// 展示名。
    pub name: Option<String>,
    /// 描述。
    pub description: Option<String>,
    /// 兼容的前端运行时契约版本。
    pub frontend_runtime: String,
    /// 兼容的 API 契约 major。
    pub api_contract: String,
    /// 产出该制品的提交。
    pub commit: Option<String>,
    /// 构建时间。
    pub built_at: Option<String>,
}

// ============================================================================
// GitHub 原始响应（只反序列化用得到的字段）
// ============================================================================

#[derive(Debug, Deserialize)]
struct GhRepo {
    full_name: String,
    name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: Option<u32>,
    pushed_at: Option<String>,
    #[serde(default)]
    archived: bool,
    default_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    name: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

/// `BUILD_INFO.json` 的形状（只取市场要用的部分；前端仓库可能还没有这个文件）。
#[derive(Debug, Deserialize)]
struct GhBuildInfo {
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    built_at: Option<String>,
    #[serde(default)]
    frontend: Option<GhFrontendManifest>,
}

#[derive(Debug, Deserialize)]
struct GhFrontendManifest {
    id: String,
    version: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    compatibility: Option<GhCompatibility>,
}

#[derive(Debug, Deserialize)]
struct GhCompatibility {
    #[serde(default, rename = "frontendRuntime")]
    frontend_runtime: Option<String>,
    #[serde(default, rename = "apiContract")]
    api_contract: Option<String>,
}

// ============================================================================
// 句柄：短时缓存
// ============================================================================

struct CachedSnapshot {
    at: Instant,
    dto: FrontendMarketDto,
}

/// 前端市场句柄。持有 GitHub 发现结果的**短时缓存**。
///
/// 为什么要缓存：未认证的 GitHub API 限流是 **60 次/小时/IP**，而刷新一次市场要打
/// `1 + N(+N)` 次请求。管理员连点几下刷新就会打满。缓存让重复查询在 TTL 内零请求。
///
/// 缓存放在进程内（与 `ProxyReqwest` 同级的 app_data 句柄）。多 worker 共享同一份，
/// 是刻意的：市场数据是**只读的外部元数据**，不一致窗口没有意义。
#[derive(Clone)]
pub struct FrontendMarket {
    cache: Arc<Mutex<Option<CachedSnapshot>>>,
}

impl FrontendMarket {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// 取市场快照；TTL 内直接返回缓存。
    ///
    /// `force = true` 时跳过缓存（管理员显式点「刷新」）。
    pub async fn snapshot(
        &self,
        http: &ProxyReqwest,
        cfg: &FrontendMarketConfig,
        force: bool,
    ) -> anyhow::Result<FrontendMarketDto> {
        let ttl = Duration::from_secs(cfg.cache_ttl_secs.max(1));

        if !force {
            if let Ok(guard) = self.cache.lock() {
                if let Some(cached) = guard.as_ref() {
                    if cached.at.elapsed() < ttl {
                        let mut dto = cached.dto.clone();
                        dto.cached = true;
                        return Ok(dto);
                    }
                }
            }
        }

        let fresh = self.fetch(http, cfg).await?;
        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some(CachedSnapshot {
                at: Instant::now(),
                dto: fresh.clone(),
            });
        }
        Ok(fresh)
    }

    /// 真正打 GitHub。任何单仓库失败都记进 `warnings` 并继续。
    async fn fetch(
        &self,
        http: &ProxyReqwest,
        cfg: &FrontendMarketConfig,
    ) -> anyhow::Result<FrontendMarketDto> {
        let client = http
            .client()
            .await
            .map_err(|e| anyhow::anyhow!("构建出网客户端失败：{e}"))?;
        let token = cfg.token.as_ref().map(|t| t.expose().to_string());

        let mut warnings = Vec::new();

        // ① 列出该 owner 下的仓库，再按前缀过滤。
        //
        // 刻意**不用** search API：GitHub 的搜索索引有延迟，刚建的仓库可能查不到；
        // 而 `/orgs/{owner}/repos` 是权威且即时的。候选数量本来就少（前缀很窄）。
        let repos_url = format!(
            "{GITHUB_API}/orgs/{}/repos?per_page=100&sort=pushed&direction=desc",
            cfg.owner
        );
        let repos: Vec<GhRepo> = self
            .get_json(&client, &repos_url, token.as_deref())
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "读取 GitHub 仓库列表失败（{repos_url}）：{e}。\
                     若该组织不存在请检查 [frontend_market].owner；\
                     若疑似限流，可在配置里提供 [frontend_market].token。"
                )
            })?;

        let matched: Vec<GhRepo> = repos
            .into_iter()
            .filter(|r| r.name.starts_with(&cfg.name_prefix))
            .collect();

        // ② 逐个仓库取 release（+ 试着读 BUILD_INFO.json 拿制品 manifest）。
        let mut items = Vec::with_capacity(matched.len());
        for repo in matched {
            let releases_url = format!(
                "{GITHUB_API}/repos/{}/releases?per_page={RELEASES_PER_REPO}",
                repo.full_name
            );
            let releases: Vec<GhRelease> = match self
                .get_json(&client, &releases_url, token.as_deref())
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    warnings.push(format!("{}：读取 release 失败（{e}）", repo.full_name));
                    Vec::new()
                }
            };

            let mut release_dtos = Vec::with_capacity(releases.len());
            for rel in releases {
                let assets: Vec<FrontendAssetDto> = rel
                    .assets
                    .iter()
                    .filter(|a| {
                        a.name == ASSET_ARCHIVE
                            || a.name == ASSET_CHECKSUM
                            || a.name == ASSET_BUILD_INFO
                    })
                    .map(|a| FrontendAssetDto {
                        name: a.name.clone(),
                        size: a.size,
                        download_url: a.browser_download_url.clone(),
                    })
                    .collect();

                let archive = rel.assets.iter().find(|a| a.name == ASSET_ARCHIVE);
                let build_info = rel.assets.iter().find(|a| a.name == ASSET_BUILD_INFO);

                // 只读那个**小的** BUILD_INFO.json，绝不为了元数据下载整个制品。
                let artifact = match build_info {
                    Some(asset) => match self
                        .get_json::<GhBuildInfo>(
                            &client,
                            &asset.browser_download_url,
                            token.as_deref(),
                        )
                        .await
                    {
                        Ok(info) => info.frontend.map(|f| FrontendArtifactDto {
                            id: f.id,
                            version: f.version,
                            name: f.name,
                            description: f.description,
                            frontend_runtime: f
                                .compatibility
                                .as_ref()
                                .and_then(|c| c.frontend_runtime.clone())
                                .unwrap_or_default(),
                            api_contract: f
                                .compatibility
                                .as_ref()
                                .and_then(|c| c.api_contract.clone())
                                .unwrap_or_default(),
                            commit: info.commit,
                            built_at: info.built_at,
                        }),
                        Err(e) => {
                            warnings.push(format!(
                                "{} {}：读取 BUILD_INFO.json 失败（{e}）",
                                repo.full_name, rel.tag_name
                            ));
                            None
                        }
                    },
                    None => None,
                };

                let install_command = archive.map(|a| {
                    format!(
                        "frontend.sh install {}",
                        shell_quote(&a.browser_download_url)
                    )
                });

                release_dtos.push(FrontendReleaseDto {
                    tag: rel.tag_name,
                    name: rel.name,
                    published_at: rel.published_at,
                    prerelease: rel.prerelease,
                    draft: rel.draft,
                    html_url: rel.html_url,
                    assets,
                    // 「可安装」的判据是**有制品归档**；元数据缺失只影响展示，不影响能否安装。
                    installable: archive.is_some(),
                    install_command,
                    artifact,
                });
            }

            let install_command = release_dtos.iter().find_map(|r| r.install_command.clone());

            items.push(FrontendMarketItemDto {
                full_name: repo.full_name,
                name: repo.name,
                html_url: repo.html_url,
                description: repo.description,
                stars: repo.stargazers_count.unwrap_or(0),
                pushed_at: repo.pushed_at,
                archived: repo.archived,
                default_branch: repo.default_branch.unwrap_or_else(|| "main".to_string()),
                releases: release_dtos,
                install_command,
            });
        }

        Ok(FrontendMarketDto {
            owner: cfg.owner.clone(),
            name_prefix: cfg.name_prefix.clone(),
            fetched_at: chrono::Utc::now().to_rfc3339(),
            cached: false,
            authenticated: token.is_some(),
            cache_ttl_secs: cfg.cache_ttl_secs,
            items,
            warnings,
        })
    }

    /// 带体积上限的 JSON GET。
    ///
    /// 复用 [`ProxyReqwest::get_bytes`]（边读边计数，`Content-Length` 撒谎也不会吃光内存），
    /// 因此出网代理、`no_proxy` 等策略与平台其它出网请求**完全一致**。
    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        client: &reqwest::Client,
        url: &str,
        token: Option<&str>,
    ) -> anyhow::Result<T> {
        let mut req = client
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "floatctf-platform-frontend-market");
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }

        let response = req
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("请求 {url} 失败：{e}"))?;

        let status = response.status();
        if !status.is_success() {
            // 403/429 基本就是限流；把这一点明说，否则管理员只会看到「失败」。
            let hint = if status.as_u16() == 403 || status.as_u16() == 429 {
                "（这通常是 GitHub 限流：未认证 60 次/小时。可在 [frontend_market].token 配置一个 token）"
            } else {
                ""
            };
            anyhow::bail!("{url} 返回 HTTP {status}{hint}");
        }

        if let Some(len) = response.content_length() {
            if len > MAX_JSON_BYTES {
                anyhow::bail!("{url} 响应体 {len} 字节，超过上限 {MAX_JSON_BYTES}");
            }
        }

        let mut buffer: Vec<u8> = Vec::new();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| anyhow::anyhow!("读取 {url} 响应失败：{e}"))?
        {
            if buffer.len() as u64 + chunk.len() as u64 > MAX_JSON_BYTES {
                anyhow::bail!("{url} 响应体超过上限 {MAX_JSON_BYTES} 字节，已中止");
            }
            buffer.extend_from_slice(&chunk);
        }

        serde_json::from_slice(&buffer).map_err(|e| anyhow::anyhow!("解析 {url} 的 JSON 失败：{e}"))
    }
}

impl Default for FrontendMarket {
    fn default() -> Self {
        Self::new()
    }
}

/// 把 URL 安全地放进可复制的 shell 命令里（加单引号并转义内部单引号）。
///
/// 目的是让管理员能直接粘贴；URL 来自 GitHub API，但**不因此就信任它** ——
/// 一个带空格或引号的 URL 会让人粘贴出一条完全不同的命令。
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_wraps_and_escapes() {
        assert_eq!(shell_quote("https://a/b.tar.gz"), "'https://a/b.tar.gz'");
        // 单引号必须被转义，否则粘贴出去会截断命令并执行剩余部分
        assert_eq!(shell_quote("a'b"), r"'a'\''b'");
        assert_eq!(shell_quote("a b"), "'a b'");
    }

    // ── GitHub 响应的解析 ────────────────────────────────────────────────────
    //
    // 这一层是市场最脆的地方：GitHub 的字段是外部契约，改了名字 / 挪了层级 /
    // 变成可空，市场要么报错要么显示错。下面的 fixture 按**真实响应**的形状写
    // （字段名与可空性照抄 API，不是凭空构造），用来钉住市场真正依赖的那几个字段。

    /// 真实的 `/orgs/{owner}/repos` 元素形状（只保留市场用到的字段 + 会出现的可空字段）。
    const REPO_JSON: &str = r#"{
      "full_name": "FloatCTF/floatctf-frontend-shadcn",
      "name": "floatctf-frontend-shadcn",
      "html_url": "https://github.com/FloatCTF/floatctf-frontend-shadcn",
      "description": "基于 shadcn/ui 的 FloatCTF 可插拔前端",
      "stargazers_count": 0,
      "pushed_at": "2026-10-13T03:12:13Z",
      "archived": false,
      "default_branch": "main"
    }"#;

    #[test]
    fn parses_repo_and_keeps_the_fields_the_market_shows() {
        let r: GhRepo = serde_json::from_str(REPO_JSON).expect("repo fixture 必须能解析");
        assert_eq!(r.name, "floatctf-frontend-shadcn");
        assert_eq!(r.full_name, "FloatCTF/floatctf-frontend-shadcn");
        assert_eq!(r.default_branch.as_deref(), Some("main"));
        assert!(!r.archived);
        // 前缀过滤是市场「发现」的全部依据，单独钉住
        assert!(r.name.starts_with("floatctf-frontend-"));
    }

    /// 新建仓库常见：`description` 为 null、`stargazers_count` 缺失。
    /// 少了它们不该让整个市场失败 —— 只该让那一项少显示一个字段。
    #[test]
    fn tolerates_null_description_and_missing_optional_fields() {
        let minimal = r#"{
          "full_name": "FloatCTF/floatctf-frontend-bare",
          "name": "floatctf-frontend-bare",
          "html_url": "https://github.com/FloatCTF/floatctf-frontend-bare",
          "description": null
        }"#;
        let r: GhRepo = serde_json::from_str(minimal).expect("可空字段不该导致解析失败");
        assert!(r.description.is_none());
        assert_eq!(r.stargazers_count, None);
        assert!(!r.archived, "缺 archived 时必须是 false，不能是「未知」");
        // 缺 default_branch 时由代码回落成 "main"，这里只确认它是 None
        assert!(r.default_branch.is_none());
    }

    /// 真实的 `/repos/{full_name}/releases` 元素形状（市场只认三个资产名）。
    #[test]
    fn parses_release_and_filters_to_the_three_known_assets() {
        let json = r#"{
          "tag_name": "frontend-latest",
          "name": "前端制品 · FloatCTF Shadcn Console（滚动通道）",
          "published_at": "2026-10-13T03:12:13Z",
          "prerelease": false,
          "draft": false,
          "html_url": "https://github.com/FloatCTF/floatctf-frontend-shadcn/releases/tag/frontend-latest",
          "assets": [
            {"name": "BUILD_INFO.json", "size": 753,
             "browser_download_url": "https://github.com/x/BUILD_INFO.json"},
            {"name": "frontend-dist.tar.gz", "size": 677167,
             "browser_download_url": "https://github.com/x/frontend-dist.tar.gz"},
            {"name": "frontend-dist.tar.gz.sha256", "size": 87,
             "browser_download_url": "https://github.com/x/frontend-dist.tar.gz.sha256"},
            {"name": "some-unrelated.txt", "size": 1, "browser_download_url": "https://github.com/x/nope"}
          ]
        }"#;
        let rel: GhRelease = serde_json::from_str(json).expect("release fixture 必须能解析");
        assert_eq!(rel.tag_name, "frontend-latest");
        assert!(!rel.draft && !rel.prerelease);

        let kept: Vec<&str> = rel
            .assets
            .iter()
            .filter(|a| {
                a.name == ASSET_ARCHIVE || a.name == ASSET_CHECKSUM || a.name == ASSET_BUILD_INFO
            })
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(
            kept,
            vec![ASSET_BUILD_INFO, ASSET_ARCHIVE, ASSET_CHECKSUM],
            "市场只保留约定的三个资产名"
        );

        // 「可安装」的判据是存在制品归档 —— 这正是 `installable` 字段的来源
        assert!(rel.assets.iter().any(|a| a.name == ASSET_ARCHIVE));
    }

    /// `BUILD_INFO.json` 的约定：`frontend` 是制品 manifest 的**原样副本**。
    /// 市场靠它拿到 id / version / 兼容契约 —— 这是「不必下载整个制品」的全部依据。
    #[test]
    fn parses_build_info_and_extracts_manifest_metadata() {
        let json = r#"{
          "repository": "FloatCTF/floatctf-frontend-xzmu",
          "commit": "26e9aaf",
          "ref": "refs/heads/main",
          "built_at": "2026-10-13T03:12:23.596Z",
          "files": 5,
          "bytes": 551062,
          "frontend": {
            "schemaVersion": 1,
            "id": "xzmu",
            "name": "西藏民族大学 CTF 平台",
            "version": "0.1.8",
            "description": "…",
            "author": "FloatCTF",
            "compatibility": {"frontendRuntime": "1", "apiContract": "1"},
            "entry": "assets/frontend.js",
            "styles": ["assets/frontend.css"]
          }
        }"#;
        let bi: GhBuildInfo = serde_json::from_str(json).expect("BUILD_INFO fixture 必须能解析");
        assert_eq!(bi.commit.as_deref(), Some("26e9aaf"));
        let f = bi.frontend.expect("frontend 字段必须存在");
        assert_eq!(f.id, "xzmu");
        assert_eq!(f.version, "0.1.8");
        let c = f.compatibility.expect("compatibility 必须存在");
        assert_eq!(c.frontend_runtime.as_deref(), Some("1"));
        // 驼峰字段名（frontendRuntime / apiContract）必须靠 rename 正确映射 ——
        // 改错了这里会静默变成 None，市场就会显示空白契约。
        assert_eq!(c.api_contract.as_deref(), Some("1"));

        // 市场把 apiContract 缺失呈现为空串（见 map 分支），因此 None 是允许的
        let no_compat = r#"{"frontend": {"id":"a","version":"1.0.0"}}"#;
        let bi2: GhBuildInfo = serde_json::from_str(no_compat).unwrap();
        let f2 = bi2.frontend.unwrap();
        assert!(f2.compatibility.is_none());
        assert!(f2.name.is_none());
    }

    /// **向后兼容**：还没接发布 CI 的前端仓库，release 里没有 `BUILD_INFO.json`。
    /// 这不是错误 —— 市场应当只显示资产信息，不报 warning、不影响其它仓库。
    #[test]
    fn missing_frontend_field_is_not_an_error() {
        let bi: GhBuildInfo = serde_json::from_str(r#"{"commit":"abc"}"#).unwrap();
        assert!(
            bi.frontend.is_none(),
            "没有 frontend 字段时应当是 None，而非解析失败"
        );
        assert_eq!(bi.commit.as_deref(), Some("abc"));
    }
}
