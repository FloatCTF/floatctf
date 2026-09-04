# crates.io 发布指南

> 状态：**已准备完成**（commit `eef5c01`），实际 `cargo publish` 由你在已登录 crates.io 的环境执行。
> 本沙箱的 `~/.cargo` 只有 rsproxy 镜像 token（`crates.io/api/v1/me` 返回 403），**不是** crates.io 发布 token。

## 一、背景发现

- **`fcmc` 已在 crates.io 发布过 1.1.0**（2026-08-11），但本地比它新了多个提交（AWD 网络避让、回程放行、GameBox 模板对齐、AWDP 相关修复等）。本次升版 **1.1.0 → 1.2.0** 重新发布，纳入全部未发布改动。
- **`floatctf` 服务端 crate 未发布过**（`floatctf` 名字在 crates.io 空闲）。本次补全 package 元数据使其可发布。
- `awd-flagserver` / `awd-judgeserver` / `awdp-judgeserver` 三个 crate 均为 `publish = false`（内部服务，不应发布），**保持不变**。
- `release.yml` 引用了不存在的 `floatctf-migration` 二进制（旧迁移体系产物，已在 `9e0df54` 移除），会导致构建 Release 时 `tar` 失败；本次已移除并补上 `awdp_judgeserver`。

## 二、本次改动（commit `eef5c01`）

| 文件 | 改动 |
|------|------|
| `crates/fcmc/Cargo.toml` | `version = "1.2.0"`；去掉 README 行尾注释 |
| `crates/fcmc/README.md` | 增加「在 crates.io 上发布」说明：`cargo install fcmc` + crates.io 链接 |
| `apps/api/Cargo.toml` | 补全 `description`（中文）/ `license.workspace` / `repository` / `readme`；`fcmc` 依赖改为 `path + version = "1.2"` |
| `Cargo.lock` | `fcmc` 锁定到 1.2.0 |
| `.github/workflows/release.yml` | 二进制约列表：`floatctf fcmc awd_flagserver awd_judgeserver awdp_judgeserver`（去掉 `floatctf-migration`） |

## 三、发布前验证（本环境已跑）

- `cargo fmt --all -- --check`：通过
- `cargo check -p floatctf`：通过（fcmc 1.2.0 path dep 编译 OK）
- `cargo test -p fcmc`：12 passed
- `cargo publish --dry-run --registry crates-io -p fcmc --allow-dirty`：Uploading fcmc v1.2.0 … dry run 通过
- `cargo package -p floatctf --list`：443 个文件，含 README/config，**不含 .env / floatctf.toml / 密钥**

> 注意：`floatctf` 服务端的 `cargo publish --dry-run` 因 `fcmc ^1.2` 尚未真正发布而无法通过依赖解析 —— **这是预期的**，必须先发布 fcmc 1.2.0，再发布 floatctf。

## 四、发布步骤（由你执行，按顺序）

在已 `cargo login`（crates.io token）的环境，于本仓库根目录：

```bash
# 1. 发布 fcmc 1.2.0（无未提交改动，无需 --allow-dirty）
cargo publish --registry crates-io -p fcmc

# 2. 等 fcmc 1.2.0 在 crates.io 生效（索引同步），再发布服务端
cargo publish --registry crates-io -p floatctf
```

- 若仓库工作区有未提交改动会报 dirty，加 `--allow-dirty`（注意：当前工作区尚有无关的 `README.md`、`apps/api/config/development.toml` 改动，发布时建议先 `git stash` 或确认不混入）。
- 建议在**干净 git 状态**（仅含本 commit）下发布，最稳妥。

## 五、发布后验证

```bash
cargo install fcmc          # 应安装到 1.2.0
fcmc --version              # → fcmc 1.2.0
```

## 六、GitHub Release（可选）

`release.yml` 现在能正确打出全部 5 个可执行文件。打 tag 触发：

```bash
git tag v0.3.3 && git push origin v0.3.3   # 触发 web + rust 两个 Release job
```

产物：`web-dist.tar.gz` + `rust-binaries.tar.gz`（含 floatctf/fcmc/awd_flagserver/awd_judgeserver/awdp_judgeserver）。