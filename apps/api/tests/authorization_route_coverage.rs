//! Wave 5 Gate：**每条 admin 路由都必须声明所需权限**（源码守卫）。
//!
//! # 为什么需要这个测试
//!
//! `RequirePermission<P>` 的价值在于「忘记声明权限」会变成**编译期/测试期失败**，
//! 而不是「登录即可用」的静默漏洞。但迁移完成后，真正的风险变成了
//! **回归**：将来有人新增一个 admin handler，顺手写下
//! `_: SuperAdminJwtGuard`（或忘了任何守卫），就绕过了逐路由授权。
//!
//! 因此这里断言：
//! 1. `SuperAdminJwtGuard` 作为**参数类型**只允许出现在文档化的遗留白名单里
//!    （`/api/admin/super_admin` CRUD，Wave 10 删除），其余位置一律不允许；
//! 2. 迁移后的 handler 数量不低于基线（防止有人整体回退）；
//! 3. 所有被用作守卫的 `<X>Guard` 都来自 `core::permission::guards` 的 29 条目录
//!    （防止出现「自造守卫类型」这种绕过权限目录的路径）。
//!
//! 这个测试**不需要数据库、不需要网络**，因此在本机也真实执行。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 允许继续使用 `SuperAdminJwtGuard` 作为参数的文件（相对于 `apps/api/src`）。
///
/// 只有 `/api/admin/super_admin` CRUD —— `platform-permissions.md` 的表中这 5 条
/// 明确标注为「旧 `SuperAdminJwtGuard`」，它们在 Wave 10 随 `super_admin` 表一起删除。
/// Wave 10 起**为空**：最后一个使用粗粒度超管守卫的模块（`identity/administrator`）
/// 已随 `super_admin` 表一起删除，全仓管理端点一律 `RequirePermission<P>`。
///
/// 特意保留这个常量（而不是删掉它和用它做过滤的断言）：它是一条"迁移已完成且不应回退"
/// 的锚点 —— 将来若有人新写一个超管守卫，这里会立刻变红而不是静默通过。
const LEGACY_GUARD_ALLOWLIST: &[&str] = &[];

/// 迁移完成后的 `RequirePermission<..>` 参数出现次数下限。
///
/// 实测值留出余量：新增端点会提高它，整体回退会立刻跌破它。
const MIN_REQUIRE_PERMISSION_PARAMS: usize = 50;

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    out
}

/// 逐行扫描，返回 `(相对路径, 行号, 行内容)`：包含 `: SuperAdminJwtGuard` 的行。
///
/// 只看**参数位置**（`: Type`），因此纯注释/文档里的提及不会被误判。
fn super_admin_guard_params() -> Vec<(String, usize, String)> {
    let root = src_root();
    let mut hits = Vec::new();
    for file in rust_files(&root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        for (idx, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if line.contains(": SuperAdminJwtGuard") {
                hits.push((rel.clone(), idx + 1, line.trim().to_string()));
            }
        }
    }
    hits
}

/// **核心不变式**：粗粒度超管守卫只能出现在遗留白名单里。
#[test]
fn only_documented_legacy_handlers_use_the_coarse_super_admin_guard() {
    let hits = super_admin_guard_params();
    let bad: Vec<String> = hits
        .iter()
        .filter(|(rel, _, _)| !LEGACY_GUARD_ALLOWLIST.contains(&rel.as_str()))
        .map(|(rel, line, text)| format!("{rel}:{line}: {text}"))
        .collect();

    assert!(
        bad.is_empty(),
        "以下 handler 仍在使用粗粒度 SuperAdminJwtGuard，必须改为具体权限的 \
         RequirePermission<P>（映射见 docs/architecture/platform-permissions.md）：\n{}",
        bad.join("\n")
    );
}

/// 遗留白名单本身必须**确实**还在用（否则说明它已经迁移完，应该把白名单清空）。
///
/// 这条反向断言防止「白名单永久存在但已无意义」——Wave 10 删除 super_admin CRUD 时
/// 这个测试会红，提醒把白名单条目一起删掉。
#[test]
fn legacy_allowlist_is_not_stale() {
    let hits = super_admin_guard_params();
    for allowed in LEGACY_GUARD_ALLOWLIST {
        assert!(
            hits.iter().any(|(rel, _, _)| rel == allowed),
            "{allowed} 已在白名单里但不再使用 SuperAdminJwtGuard —— \
             请把它从 LEGACY_GUARD_ALLOWLIST 中移除（白名单不应长期保留死条目）"
        );
    }
}

/// 迁移规模下限：整体回退会让 `RequirePermission` 参数数量骤降。
#[test]
fn require_permission_coverage_is_not_regressed() {
    let root = src_root();
    let mut count = 0usize;
    let mut files = BTreeSet::new();
    for file in rust_files(&root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if line.contains(": RequirePermission<") {
                count += 1;
                files.insert(
                    file.strip_prefix(&root)
                        .unwrap_or(&file)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    assert!(
        count >= MIN_REQUIRE_PERMISSION_PARAMS,
        "RequirePermission 参数数量 {count} 低于基线 {MIN_REQUIRE_PERMISSION_PARAMS} \
         —— 疑似发生整体回退（涉及 {} 个文件）",
        files.len()
    );
    // 至少覆盖多个模块，避免「只有一个文件被迁移」也算通过
    assert!(
        files.len() >= 15,
        "RequirePermission 只出现在 {} 个文件中，覆盖面异常",
        files.len()
    );
}

/// 所有用作守卫的 `<X>Guard` 必须来自权限目录（29 条），不得自造。
#[test]
fn every_guard_type_comes_from_the_permission_catalog() {
    let root = src_root();
    let catalog = std::fs::read_to_string(root.join("core/permission.rs"))
        .expect("core/permission.rs 必须存在");
    // 宏里每个权限都由 `$guard` 占位符生成；权威名单是宏调用里的 29 个名字
    let known: BTreeSet<String> = regex_lite_guards(&catalog);

    assert!(
        !known.is_empty(),
        "未能从 core/permission.rs 解析出任何 guard 名称 —— 解析逻辑需要更新"
    );

    let mut unknown = BTreeSet::new();
    for file in rust_files(&root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for name in extract_guard_names(line) {
                if !known.contains(&name) {
                    unknown.insert(name);
                }
            }
        }
    }

    assert!(
        unknown.is_empty(),
        "发现权限目录之外的守卫类型 {unknown:?} —— 守卫必须由 define_permissions! 生成，\
         否则「有权限却没有守卫」这类漂移就会重新出现"
    );
}

/// 从 `RequirePermission<X>` / `RequireAny<X, Y>` 里抽取守卫名。
fn extract_guard_names(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in ["RequirePermission<", "RequireAny<"] {
        let mut rest = line;
        while let Some(i) = rest.find(marker) {
            rest = &rest[i + marker.len()..];
            let Some(end) = rest.find('>') else { break };
            for part in rest[..end].split(',') {
                let name = part.trim();
                if name.ends_with("Guard") {
                    out.push(name.to_string());
                }
            }
            rest = &rest[end..];
        }
    }
    out
}

/// 从宏调用里解析 `$guard` 名字（形如 `UsersReadGuard;`）。
///
/// 不引入 regex 依赖：直接按行找「以 `Guard;` 结尾且形如标识符」的片段。
fn regex_lite_guards(catalog: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for raw in catalog.split(|c: char| c == ',' || c == '\n' || c == ';' || c == '{' || c == '}') {
        let token = raw.trim();
        if token.ends_with("Guard")
            && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && token.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            out.insert(token.to_string());
        }
    }
    out
}
