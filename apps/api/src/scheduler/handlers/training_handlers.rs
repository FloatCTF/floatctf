//! `platform.training.sync`：训练站（`FloatCTF/floatctf-training`）静态制品同步。
//!
//! 平台**只做下载 → 校验 → 安全解包 → 原子切换**，绝不重新构建：制品由训练站 CI 在
//! `main` 上产出（GitHub Release 滚动通道 `training-latest`，见该仓库
//! `.github/workflows/release.yml`）。因此生产机不需要 Node，也不执行对方的仓库代码。
//!
//! 目录布局（`{{WORK_DIR}}/training`，生产即 `$FLOATCTF_HOME/runtime/training`，
//! Caddy 以只读方式挂在 `/srv` 下）：
//!
//! ```text
//! training/
//!   current -> releases/<sha>      # 相对软链；切换是原子的，改软链无需重启 Caddy
//!   releases/<sha>/...             # 每次同步一个新目录
//!   REVISION                       # 当前生效的提交
//! ```
//!
//! 失败语义：任何一步出错都**不动 `current`**，线上继续跑旧版本；错误上抛给调度引擎，
//! 管理端 `/admin/scheduled_tasks` 能看到 failed 与原因。

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::{
    entity::scheduled_tasks,
    infrastructure::{
        ProxyReqwest,
        settings::{
            TRAINING_SITE_DIR_SETTING_KEY, TRAINING_SITE_REVISION_SETTING_KEY, get_setting,
            resolve_dir_path, upsert_setting,
        },
    },
    scheduler::{TaskHandler, TaskKey},
};

/// 训练站制品的发布通道（滚动 release，永远指向对方 `main` 的最新构建）。
const RELEASE_CHANNEL_BASE: &str =
    "https://github.com/FloatCTF/floatctf-training/releases/download/training-latest";

const ASSET_BUILD_INFO: &str = "BUILD_INFO.json";
const ASSET_ARCHIVE: &str = "training-dist.tar.gz";
const ASSET_CHECKSUM: &str = "training-dist.tar.gz.sha256";

/// 制品必须按这个前缀构建（与 Caddy 的 `/training/*` 路由一致）；否则站内链接全断，
/// 宁可同步失败也不要上线一个必然 404 的站点。
const EXPECTED_BASE: &str = "/training/";

/// 同步互斥锁的键（Postgres advisory lock，跨进程）。
const SYNC_LOCK_KEY: &str = "floatctf-platform-training-sync";

/// 下载体积上限：BUILD_INFO 很小，归档按当前体积（约 21 MB）+ 充足余量。
const MAX_BUILD_INFO_BYTES: u64 = 64 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 4 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
/// 解包上限：条目数与解压后总字节，防止解压炸弹。
const MAX_ENTRIES: usize = 20_000;
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// 保留的历史版本数（含当前版本）。
const KEEP_RELEASES: usize = 3;

/// 制品自带的构建元数据（由训练站 CI 写出，字段见其 `release.yml`）。
#[derive(Debug, Deserialize)]
struct BuildInfo {
    commit: String,
    /// 构建时使用的子路径前缀。
    #[serde(default)]
    base: String,
}

pub struct TrainingSyncHandler {
    pub db: crate::infrastructure::WebDb,
    pub proxy: ProxyReqwest,
}

#[async_trait]
impl TaskHandler for TrainingSyncHandler {
    fn trigger_type(&self) -> &'static str {
        "startup"
    }

    fn task_key(&self) -> TaskKey {
        TaskKey::PlatformTrainingSync
    }

    async fn run(&self, _task: scheduled_tasks::Model) -> anyhow::Result<()> {
        // 互斥：引擎的启动路径（`init_and_recover` 直接 spawn startup 任务，**不认领行**）
        // 与 5s 轮询会并发派发同一个任务。本任务写共享目录（暂存/改名/软链），并发跑会
        // 互相删掉对方的暂存目录，最坏情况是 `current` 指向一个已被删除的 release → 站点 404。
        // 取不到锁说明已有一次同步在跑，直接跳过即可（不是错误）。
        let Some(lock) = SyncLock::try_acquire(self.db.get_ref(), SYNC_LOCK_KEY).await? else {
            info!("另一个训练站同步正在进行，跳过本次派发");
            return Ok(());
        };
        let result = self.sync_once().await;
        lock.release().await;
        result
    }
}

impl TrainingSyncHandler {
    async fn sync_once(&self) -> anyhow::Result<()> {
        let root = self.site_root().await?;
        let releases = root.join("releases");
        std::fs::create_dir_all(&releases)
            .map_err(|e| anyhow::anyhow!("创建 {} 失败：{e}", releases.display()))?;

        let info = self.fetch_build_info().await?;
        if info.base != EXPECTED_BASE {
            anyhow::bail!(
                "训练站制品的 TRAINING_BASE 是 {:?}，与平台路由 {EXPECTED_BASE} 不一致：\
                 站内链接会全部 404，已放弃本次同步（请检查训练站仓库变量 TRAINING_BASE）",
                info.base
            );
        }
        let revision = info.commit.trim().to_string();
        if revision.is_empty() {
            anyhow::bail!("训练站 BUILD_INFO.json 缺少 commit 字段");
        }

        let current_link = root.join("current");
        if self.current_revision().await.as_deref() == Some(revision.as_str())
            && current_link.exists()
        {
            info!(revision = %revision, "训练站已是最新，跳过同步");
            return Ok(());
        }

        let proxy = self.proxy.describe_proxy().await;
        info!(revision = %revision, proxy = %proxy, "开始同步训练站制品");
        let archive = self
            .proxy
            .get_bytes(&self.asset_url(ASSET_ARCHIVE), MAX_ARCHIVE_BYTES)
            .await?;
        let expected_sha = self.fetch_checksum().await?;
        let actual_sha = hex::encode(Sha256::digest(&archive));
        if actual_sha != expected_sha {
            anyhow::bail!(
                "训练站制品 sha256 校验失败：期望 {expected_sha}，实际 {actual_sha}（已放弃本次同步）"
            );
        }

        // 解包到暂存目录，全部通过后才改名成正式 release 目录。
        let staging = releases.join(format!(".staging-{revision}"));
        let target = releases.join(&revision);
        remove_dir_if_exists(&staging)?;
        let extract_result = extract_archive(&archive, &staging);
        if let Err(error) = extract_result {
            let _ = remove_dir_if_exists(&staging);
            return Err(error);
        }
        if let Err(error) = verify_site_layout(&staging) {
            let _ = remove_dir_if_exists(&staging);
            return Err(error);
        }

        remove_dir_if_exists(&target)?;
        std::fs::rename(&staging, &target).map_err(|e| {
            anyhow::anyhow!(
                "暂存目录改名失败 {} → {}：{e}",
                staging.display(),
                target.display()
            )
        })?;

        // 原子切换：先建临时软链再 rename 覆盖，避免出现"current 短暂不存在"的窗口。
        switch_current(&root, Path::new("releases").join(&revision).as_path())?;

        let revision_file = root.join("REVISION");
        std::fs::write(&revision_file, format!("{revision}\n"))
            .map_err(|e| anyhow::anyhow!("写入 {} 失败：{e}", revision_file.display()))?;
        if let Err(error) = upsert_setting(
            self.db.get_ref(),
            TRAINING_SITE_REVISION_SETTING_KEY,
            &revision,
        )
        .await
        {
            // 记录失败不影响本次同步结果，只影响管理端可见性。
            warn!(error = %error, "写入 TRAINING_SITE_REVISION 设置失败（站点已切换）");
        }

        let pruned = prune_old_releases(&releases, &revision).unwrap_or_else(|error| {
            warn!(error = %error, "清理历史 release 失败（不影响站点）");
            0
        });
        info!(
            revision = %revision,
            dir = %target.display(),
            pruned,
            "训练站制品已切换到新版本"
        );
        Ok(())
    }

    /// 站点根目录：`{{WORK_DIR}}/training`，经设置表解析（与 CHALLENGES_DIR 同一惯例）。
    async fn site_root(&self) -> anyhow::Result<PathBuf> {
        let configured = get_setting(self.db.get_ref(), TRAINING_SITE_DIR_SETTING_KEY)
            .await
            .map_err(|e| anyhow::anyhow!("读取 {TRAINING_SITE_DIR_SETTING_KEY} 失败：{e}"))?;
        Ok(resolve_dir_path(&configured))
    }

    /// 当前生效的提交：优先读 `REVISION` 文件，缺失时回落设置表。
    async fn current_revision(&self) -> Option<String> {
        if let Ok(root) = self.site_root().await {
            if let Ok(text) = std::fs::read_to_string(root.join("REVISION")) {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
        get_setting(self.db.get_ref(), TRAINING_SITE_REVISION_SETTING_KEY)
            .await
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn asset_url(&self, asset: &str) -> String {
        format!("{RELEASE_CHANNEL_BASE}/{asset}")
    }

    async fn fetch_build_info(&self) -> anyhow::Result<BuildInfo> {
        let bytes = self
            .proxy
            .get_bytes(&self.asset_url(ASSET_BUILD_INFO), MAX_BUILD_INFO_BYTES)
            .await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("解析 {ASSET_BUILD_INFO} 失败：{e}"))
    }

    /// 校验和文件形如 `"<64 hex>  training-dist.tar.gz\n"`，取第一个字段。
    async fn fetch_checksum(&self) -> anyhow::Result<String> {
        let bytes = self
            .proxy
            .get_bytes(&self.asset_url(ASSET_CHECKSUM), MAX_CHECKSUM_BYTES)
            .await?;
        let text = String::from_utf8(bytes)
            .map_err(|e| anyhow::anyhow!("{ASSET_CHECKSUM} 不是 UTF-8：{e}"))?;
        let checksum = text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if checksum.len() != 64 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("{ASSET_CHECKSUM} 内容不是合法的 sha256：{text:?}");
        }
        Ok(checksum)
    }
}

/// 解包 tar.gz 到 `dest`：拒绝绝对路径、`..`、以及符号链接/硬链接/设备等条目类型。
///
/// 归档来自外部仓库，必须按不可信输入处理：只要允许一条 `../x` 或符号链接，就能写到
/// Caddy 会伺服的目录之外。
fn extract_archive(bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)
        .map_err(|e| anyhow::anyhow!("创建 {} 失败：{e}", dest.display()))?;

    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(false);
    archive.set_preserve_mtime(false);

    let mut entries = 0usize;
    let mut unpacked: u64 = 0;
    for entry in archive
        .entries()
        .map_err(|e| anyhow::anyhow!("读取归档条目失败：{e}"))?
    {
        let mut entry = entry.map_err(|e| anyhow::anyhow!("读取归档条目失败：{e}"))?;
        entries += 1;
        if entries > MAX_ENTRIES {
            anyhow::bail!("归档条目数超过上限 {MAX_ENTRIES}，已中止解包");
        }

        let path = entry
            .path()
            .map_err(|e| anyhow::anyhow!("归档条目路径不可解析：{e}"))?
            .to_path_buf();
        ensure_safe_relative_path(&path)?;

        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            anyhow::bail!(
                "归档含不支持的条目类型（符号链接/硬链接/设备等）：{}",
                path.display()
            );
        }

        unpacked = unpacked.saturating_add(entry.header().size().unwrap_or(0));
        if unpacked > MAX_UNPACKED_BYTES {
            anyhow::bail!("归档解压后体积超过上限 {MAX_UNPACKED_BYTES} 字节，已中止解包");
        }

        // `unpack_in` 自身也会拒绝逃逸目标目录的路径；上面的校验是为了给出更清晰的错误。
        entry
            .unpack_in(dest)
            .map_err(|e| anyhow::anyhow!("解包 {} 失败：{e}", path.display()))?;
    }
    Ok(())
}

/// 只允许普通相对路径（允许 `./` 前缀，归档是用 `tar -C dist .` 打的）。
fn ensure_safe_relative_path(path: &Path) -> anyhow::Result<()> {
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("归档条目路径不安全（含绝对路径或 ..）：{}", path.display());
            }
        }
    }
    Ok(())
}

/// 站点完整性硬检查：缺任何一项都说明制品不完整（字体与 Pagefind 索引都是生成物）。
fn verify_site_layout(dir: &Path) -> anyhow::Result<()> {
    for required in [
        "index.html",
        "404.html",
        "pagefind/pagefind.js",
        "fontsource/fonts.css",
    ] {
        let path = dir.join(required);
        if !path.is_file() {
            anyhow::bail!("制品缺少 {required}（{}），已放弃本次同步", path.display());
        }
    }
    Ok(())
}

/// 原子切换 `current` 软链：先写临时软链，再 `rename` 覆盖。
fn switch_current(root: &Path, target: &Path) -> anyhow::Result<()> {
    let link = root.join("current");
    let temporary = root.join(".current.tmp");
    let _ = std::fs::remove_file(&temporary);
    std::os::unix::fs::symlink(target, &temporary)
        .map_err(|e| anyhow::anyhow!("创建软链 {} 失败：{e}", temporary.display()))?;
    std::fs::rename(&temporary, &link)
        .map_err(|e| anyhow::anyhow!("切换 {} 失败：{e}", link.display()))?;
    Ok(())
}

/// 保留最近 [`KEEP_RELEASES`] 个版本，且绝不删除当前版本。返回删除数量。
fn prune_old_releases(releases: &Path, keep_revision: &str) -> anyhow::Result<usize> {
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(releases)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.file_type()?.is_dir() || name.starts_with('.') {
            continue;
        }
        if name == keep_revision {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        candidates.push((modified, entry.path()));
    }
    candidates.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));

    let mut removed = 0;
    for (_, path) in candidates.into_iter().skip(KEEP_RELEASES.saturating_sub(1)) {
        match std::fs::remove_dir_all(&path) {
            Ok(()) => removed += 1,
            Err(error) => warn!(dir = %path.display(), error = %error, "删除历史 release 失败"),
        }
    }
    Ok(removed)
}

fn remove_dir_if_exists(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow::anyhow!("删除 {} 失败：{error}", path.display())),
    }
}

/// 同步互斥锁：Postgres **session** advisory lock（跨进程，和 AWDP 实例锁同一范式）。
///
/// 锁绑定到独占连接；正常路径显式 [`SyncLock::release`]，未释放（提前 return / panic /
/// 被取消）时由 `Drop` 交后台任务解锁——**绝不能**把仍持有 session 锁的连接直接还回池，
/// 否则那把锁会留在池里的连接上，后续永远拿不到。
struct SyncLock {
    conn: Option<sea_orm::sqlx::pool::PoolConnection<sea_orm::sqlx::Postgres>>,
    key: String,
}

impl SyncLock {
    /// 尝试取锁；返回 `Ok(None)` 表示别的同步正在跑（调用方应跳过，而非报错）。
    async fn try_acquire(
        db: &sea_orm::DatabaseConnection,
        key: &str,
    ) -> anyhow::Result<Option<Self>> {
        let mut conn = db
            .get_postgres_connection_pool()
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!("获取数据库连接失败：{e}"))?;
        let acquired: bool = sea_orm::sqlx::query_scalar(
            "SELECT pg_try_advisory_lock(hashtextextended($1::text, 0))",
        )
        .bind(key)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| anyhow::anyhow!("pg_try_advisory_lock 失败：{e}"))?;

        if acquired {
            Ok(Some(Self {
                conn: Some(conn),
                key: key.to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn release(mut self) {
        if let Some(mut conn) = self.conn.take() {
            if let Err(error) = unlock(&mut conn, &self.key).await {
                // 解锁失败 → 该连接不能还回池，直接关闭会话（关闭即释放 session 锁）。
                warn!(error = %error, "advisory unlock 失败，关闭该连接");
                let _ = conn.close().await;
            }
        }
    }
}

async fn unlock(
    conn: &mut sea_orm::sqlx::pool::PoolConnection<sea_orm::sqlx::Postgres>,
    key: &str,
) -> Result<(), sea_orm::sqlx::Error> {
    sea_orm::sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1::text, 0))")
        .bind(key)
        .execute(&mut **conn)
        .await
        .map(|_| ())
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        if let Some(mut conn) = self.conn.take() {
            let key = self.key.clone();
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    handle.spawn(async move {
                        if let Err(error) = unlock(&mut conn, &key).await {
                            warn!(error = %error, "advisory unlock 失败，关闭该连接");
                            let _ = conn.close().await;
                        }
                    });
                }
                Err(_) => {
                    // 没有运行时：连接随 drop 归还前先关闭，避免把锁留在池里。
                    warn!("无 tokio 运行时，直接关闭持锁连接");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, contents) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *contents).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn full_site_entries() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("./index.html", b"home" as &[u8]),
            ("./404.html", b"not found"),
            ("./pagefind/pagefind.js", b"// pagefind"),
            ("./fontsource/fonts.css", b"/* fonts */"),
        ]
    }

    #[test]
    fn safe_paths_are_accepted_and_escapes_rejected() {
        for ok in ["./index.html", "a/b/c.js", "./pagefind/pagefind.js"] {
            assert!(
                ensure_safe_relative_path(Path::new(ok)).is_ok(),
                "{ok} 应当被接受"
            );
        }
        for bad in ["../evil", "a/../../evil", "/etc/passwd", "a/../../../etc"] {
            assert!(
                ensure_safe_relative_path(Path::new(bad)).is_err(),
                "{bad} 应当被拒绝"
            );
        }
    }

    #[test]
    fn extracts_a_well_formed_archive() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("site");
        extract_archive(&tar_gz(&full_site_entries()), &dest).unwrap();
        assert!(dest.join("index.html").is_file());
        assert!(dest.join("pagefind/pagefind.js").is_file());
        verify_site_layout(&dest).unwrap();
    }

    /// 手工拼 tar 头（**不经过** `Builder::set_path` 的路径校验）来模拟真正恶意的归档：
    /// tar-rs 的 builder 会拒绝含 `..` 的路径，用它编不出这种归档。
    fn raw_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar_bytes: Vec<u8> = Vec::new();
        for (name, data) in entries {
            let mut header = [0u8; 512];
            let name_bytes = name.as_bytes();
            header[..name_bytes.len()].copy_from_slice(name_bytes);
            header[100..108].copy_from_slice(b"0000644\0"); // mode
            header[108..116].copy_from_slice(b"0000000\0"); // uid
            header[116..124].copy_from_slice(b"0000000\0"); // gid
            header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
            header[136..148].copy_from_slice(b"00000000000\0"); // mtime
            header[148..156].copy_from_slice(b"        "); // chksum 占位
            header[156] = b'0'; // 普通文件
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
            header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());

            tar_bytes.extend_from_slice(&header);
            tar_bytes.extend_from_slice(data);
            let padding = (512 - data.len() % 512) % 512;
            tar_bytes.extend(std::iter::repeat_n(0u8, padding));
        }
        tar_bytes.extend(std::iter::repeat_n(0u8, 1024));

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn rejects_archive_with_escaping_entry() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("site");
        let archive = raw_tar_gz(&[("../evil.html", b"pwned" as &[u8])]);
        let error = extract_archive(&archive, &dest).expect_err("必须拒绝逃逸路径");
        assert!(error.to_string().contains("不安全"), "{error}");
        assert!(!dir.path().join("evil.html").exists());
    }

    #[test]
    fn rejects_archive_with_symlink_entry() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("site");

        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_cksum();
        builder
            .append_link(&mut header, "./link", "/etc/passwd")
            .unwrap();
        let archive = builder.into_inner().unwrap().finish().unwrap();

        let error = extract_archive(&archive, &dest).expect_err("必须拒绝符号链接条目");
        assert!(error.to_string().contains("不支持的条目类型"), "{error}");
    }

    #[test]
    fn incomplete_artifact_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("site");
        extract_archive(&tar_gz(&[("./index.html", b"home" as &[u8])]), &dest).unwrap();
        let error = verify_site_layout(&dest).expect_err("缺字体/搜索索引必须拒绝");
        assert!(error.to_string().contains("404.html"), "{error}");
    }

    #[test]
    fn prune_keeps_current_and_newest_releases() {
        let dir = tempfile::tempdir().unwrap();
        let releases = dir.path().join("releases");
        // 依次创建，mtime 递增 → "ddd" 最新
        for name in ["aaa", "bbb", "ccc", "ddd", "current-one"] {
            let path = releases.join(name);
            std::fs::create_dir_all(&path).unwrap();
            let mut handle = std::fs::File::create(path.join("index.html")).unwrap();
            handle.write_all(b"x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let removed = prune_old_releases(&releases, "current-one").unwrap();
        // 候选 4 个（当前版本被排除），保留 2 个最新 → 删 2 个；加上当前版本共留 3 个。
        assert_eq!(removed, 2);
        assert!(releases.join("current-one").is_dir(), "当前版本绝不能被删");
        assert!(releases.join("ddd").is_dir(), "最新版本必须保留");
        assert!(releases.join("ccc").is_dir(), "次新版本必须保留");
        assert!(!releases.join("aaa").is_dir());
        assert!(!releases.join("bbb").is_dir());
    }
}
