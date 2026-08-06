你这个 FloatCTF 最适合做成一个真正的 Monorepo，核心原则是：**应用放 `apps/`，共享 Rust 包放 `crates/`，部署配置放 `infra/`，仓库级脚本放 `scripts/`，整个仓库只有一个 `.git`。**

推荐结构：

```text
floatctf/
├── apps/
│   ├── api/                       # 原 floatctf-api
│   └── web/                       # 原 floatctf-web
│
├── crates/
│   └── fcmc/                      # 共享 Rust crate / CLI / SDK
│
├── infra/
│   ├── compose/
│   │   ├── compose.dev.yml
│   │   └── compose.prod.yml
│   ├── nginx/
│   │   └── nginx.conf.template
│   ├── postgres/
│   │   └── init/
│   ├── systemd/
│   └── env/
│       ├── dev.env.example
│       └── prod.env.example
│
├── scripts/
│   ├── bootstrap.sh
│   ├── dev.sh
│   ├── check.sh
│   ├── install-host.sh
│   ├── generate-cert.sh
│   └── package-release.sh
│
├── docs/
├── Cargo.toml
├── Cargo.lock
├── package.json
├── pnpm-workspace.yaml
├── pnpm-lock.yaml
├── README.md
├── LICENSE
└── .gitignore
```

## 1. 为什么目录叫 `apps/api` 和 `apps/web`

目录使用短名：

```text
apps/api
apps/web
```

仓库本身已经叫 `floatctf`，目录继续写成：

```text
apps/floatctf-api
apps/floatctf-web
```

会产生重复信息。

目录名和产物名分开：

```text
目录             apps/api
Rust package     floatctf-api
Docker image     docker.floatctf.local/floatctf/api
systemd service  floatctf-api.service
```

`apps` 使用复数，因为里面是一组可部署应用。`app` 通常表示单个应用目录。

## 2. `apps`、`crates` 的边界

`apps/` 放可以独立运行和部署的程序：

```text
apps/api
apps/web
apps/admin
apps/worker
apps/judge-server
apps/flag-server
```

`crates/` 放被其他模块复用的 Rust 包：

```text
crates/fcmc
crates/domain
crates/database
crates/protocol
crates/common
```

判断标准：

```text
可以单独部署 → apps
主要被其他 Rust 模块依赖 → crates
```

如果 `fcmc` 本身是独立 CLI，同时又被 API 作为库使用，可以保持一个 crate，通过 `src/lib.rs` 和 `src/main.rs` 同时提供库和二进制。

## 3. 根 `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = [
    "apps/api",
    "crates/fcmc",
]

[workspace.package]
edition = "2024"
license = "MIT"
repository = "https://github.com/FloatCTF/floatctf"

[workspace.dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

子项目引用共享依赖：

```toml
[package]
name = "floatctf-api"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
fcmc = { path = "../../crates/fcmc" }
```

根目录只保留一份：

```text
Cargo.lock
```

工作区成员里的旧 `Cargo.lock` 应删除，除非某个目录是刻意保持独立发布的完整 Rust workspace。

## 4. `pnpm-workspace.yaml`

```yaml
packages:
  - "apps/web"
  - "packages/*"
```

目前只有前端时，最小配置也可以是：

```yaml
packages:
  - "apps/web"
```

未来共享前端包：

```text
packages/ui
packages/eslint-config
packages/tsconfig
packages/api-client
```

再扩展：

```yaml
packages:
  - "apps/*"
  - "packages/*"
```

## 5. 根 `package.json`

```json
{
  "name": "floatctf",
  "private": true,
  "packageManager": "pnpm@10.0.0",
  "scripts": {
    "dev:web": "pnpm --filter @floatctf/web dev",
    "build:web": "pnpm --filter @floatctf/web build",
    "lint:web": "pnpm --filter @floatctf/web lint",
    "check": "./scripts/check.sh"
  }
}
```

`apps/web/package.json`：

```json
{
  "name": "@floatctf/web",
  "private": true,
  "version": "0.1.0"
}
```

根目录只保留一份：

```text
pnpm-lock.yaml
```

删除 `apps/web` 原来的 `pnpm-lock.yaml`。

## 6. `infra/` 放什么

`infra/` 保存声明式部署配置，内容通常会被 Docker、Nginx、systemd、PostgreSQL 等直接读取。

```text
infra/compose/
```

放开发和生产 Compose：

```text
compose.dev.yml
compose.prod.yml
```

```text
infra/nginx/
```

放反向代理配置、模板、证书路径约定。

```text
infra/postgres/init/
```

放数据库初始化 SQL、角色创建、扩展启用。

```text
infra/systemd/
```

放：

```text
floatctf-api.service
floatctf-worker.service
```

```text
infra/env/
```

只放示例：

```text
dev.env.example
prod.env.example
```

真实密码和密钥不进入 Git。

## 7. `scripts/` 放什么

`scripts/` 放主动执行的仓库级自动化。

```text
bootstrap.sh
```

安装项目依赖、初始化开发环境。

```text
dev.sh
```

启动开发环境，例如 PostgreSQL、API、Web。

```text
check.sh
```

统一执行：

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint
pnpm test
pnpm build
```

```text
install-host.sh
```

部署到宿主机、安装 systemd unit。

```text
generate-cert.sh
```

生成本地开发证书。

```text
package-release.sh
```

构建发布包、镜像或离线交付包。

区分原则：

```text
配置文件 → infra
执行动作 → scripts
```

## 8. 原来的 `floatctf` 仓库怎么处理

如果原 `floatctf` 仓库主要放 Compose、Nginx、启动脚本和部署配置，就把它作为新的 Monorepo 根仓库。

迁移结果：

```text
原 floatctf-api → apps/api
原 floatctf-web → apps/web
原 fcmc         → crates/fcmc
原 floatctf 部署内容 → infra、scripts、docs
```

不要再保留一个嵌套的：

```text
apps/floatctf
```

## 9. Git 仓库怎么迁

最终整个项目只保留：

```text
floatctf/.git
```

下面这些都不应存在：

```text
apps/api/.git
apps/web/.git
crates/fcmc/.git
```

你希望保留旧仓库历史时，使用 `git subtree` 导入。

API：

```bash
git remote add api-origin https://github.com/FloatCTF/floatctf-api.git
git fetch api-origin --prune

git subtree add \
  --prefix=apps/api \
  api-origin main
```

Web：

```bash
git remote add web-origin https://github.com/FloatCTF/floatctf-web.git
git fetch web-origin --prune

git subtree add \
  --prefix=apps/web \
  web-origin main
```

FCMC：

```bash
git remote add fcmc-origin https://github.com/FloatCTF/fcmc.git
git fetch fcmc-origin --prune

git subtree add \
  --prefix=crates/fcmc \
  fcmc-origin main
```

迁移结束再移除临时远程：

```bash
git remote remove api-origin
git remote remove web-origin
git remote remove fcmc-origin
```

检查：

```bash
find . -name .git -print
```

预期只有：

```text
./.git
```

## 10. 多分支如何处理

先选一个最终基线分支，例如 `main`。

旧 API 仓库中的：

```text
dev
awd
feature/*
```

先检查是否存在未合并提交：

```bash
git log --oneline api-origin/main..api-origin/dev
git log --oneline api-origin/main..api-origin/awd
```

最干净的流程是：

1. 在旧仓库把有价值的分支合并到基线；
2. 只把基线迁入 Monorepo；
3. 迁移后所有新分支从 Monorepo 创建。

迁移后的分支属于整个仓库：

```text
feature/api-auth
feature/web-scoreboard
feature/awd-mode
```

不再存在只属于一个子仓库的全局 `dev` 分支概念。

## 11. Docker 镜像命名

建议统一：

```text
docker.floatctf.local/floatctf/api:<version>
docker.floatctf.local/floatctf/web:<version>
docker.floatctf.local/floatctf/fcmc:<version>
```

构建上下文：

```bash
docker build \
  -t docker.floatctf.local/floatctf/api:dev \
  -f apps/api/Dockerfile \
  .
```

Dockerfile 可以从仓库根目录访问共享 crate：

```dockerfile
COPY Cargo.toml Cargo.lock ./
COPY apps/api apps/api
COPY crates/fcmc crates/fcmc
```

这也是 Monorepo 构建时推荐使用根目录作为 context 的原因。

## 12. CI 建议

CI 可以统一检查整个仓库，同时根据路径过滤减少无关构建：

```text
apps/api/** 或 crates/** 变化
→ Rust 检查、API 镜像

apps/web/** 变化
→ 前端检查、Web 镜像

infra/** 变化
→ Compose 和部署配置校验
```

但根工作区依赖文件变化时，应全部执行：

```text
Cargo.toml
Cargo.lock
package.json
pnpm-lock.yaml
```

## 13. 应避免的结构

避免：

```text
apps/api/.git
apps/web/.git
```

避免用 Git submodule 假装 Monorepo。

避免每个 Rust crate 各有一份锁文件。

避免每个前端包各有一份 pnpm 锁文件。

避免把部署脚本、Nginx 配置和源代码混在 `apps/api`。

避免建立过多空目录和抽象层。共享代码出现两处以上并且职责稳定后，再抽到 `crates/` 或 `packages/`。

你现在最适合先完成这四步：

```text
1. 确定原 API 的最终基线分支
2. 用 git subtree 导入 apps/api
3. 用 git subtree 导入 apps/web
4. 建立根 Cargo 和 pnpm workspace
```

最终采用 `apps/api + apps/web + crates/fcmc + infra + scripts`，这是当前 FloatCTF 最清晰、维护成本最低的结构。
