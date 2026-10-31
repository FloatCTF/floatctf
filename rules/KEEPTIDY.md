---
id: keeptidy
name: KEEPTIDY
description: Coding Agent engineering hygiene and resource management rules (project-local tmp, task isolation, cache governance, resource cleanup, Git safety, and process management)
postInstall:
  - gitignore
  - agents
---

# KEEPTIDY — Engineering Hygiene and Resource Management Rules

Applies to all Coding Agents working in the current project and their delegated Subagents, including Codex, Claude Code, Pi, DeepSeek Harness, and others.

**Core principle: Keep the working environment tidy, resources controlled, changes safe, and work traceable. Keep all controllable temporary work under the project's `tmp/` directory, and safely reclaim task-owned resources when the task ends.**

This document uses the following normative terms:

- **MUST**: Mandatory requirement.
- **MUST NOT**: Strict prohibition.
- **SHOULD**: Default requirement that may be adjusted for a justified reason.

Keep enforcement lightweight, automated where appropriate, and minimally invasive. Avoid introducing unnecessary engineering complexity solely to manage resources.

## 1. Work Scope and Project Root

- **MUST** identify the current project root (`PROJECT_ROOT`) and the task's authorized scope before taking action.
- **MUST** prefer the current Git worktree root as `PROJECT_ROOT`; for non-Git projects, use the root specified by the user or clearly identified for the project.
- **MUST** preserve the user's existing files and uncommitted changes, running services, and resources belonging to other tasks.
- **MUST** verify that the actual target path is within the authorized scope before modifying, moving, or deleting files.
- **MUST NOT** modify other repositories, the user's home-directory configuration, system directories, or files outside the task scope without authorization.
- **MUST NOT** bypass work-scope restrictions through symbolic links, path traversal, or other means.
- **SHOULD** reuse the project's existing directories, toolchains, build strategies, and engineering conventions.
- **SHOULD** keep changes within the reasonable scope of the current task and avoid unrelated refactoring.

Reading necessary system toolchains, dependencies, and external read-only resources is permitted under the project's established conventions. Writes and cleanup operations remain subject to authorization boundaries.

If the project itself is located under the system `/tmp/`, **MUST** stop file operations that conflict with these rules and report the reason.

## 2. Temporary Directories and Workspace

### 2.1 System `/tmp/` Prohibition

- **MUST NOT** actively read, list, write, modify, clean up, or delete the system `/tmp/` directory or its contents.
- **MUST NOT** actively use the system `/tmp/` to store build artifacts, test data, debug logs, downloads, caches, scripts, or databases.
- **MUST NOT** circumvent these restrictions using symbolic links, relative paths, or path redirection.
- **MUST** configure project-local temporary directories before launching controllable build, test, download, or debugging subprocesses.
- **MUST** prefer tools and execution methods that allow the temporary directory to be configured.
- If a step cannot be redirected and requires the Agent to actively operate on system `/tmp/`, **MUST** stop that step and report the blocker.
- **MUST NOT** empty the system `/tmp/` or perform global temporary-directory cleanup in response to an error.

These rules govern filesystem operations actively initiated by the Agent or configurable by it. Accurately disclose limitations involving uncontrollable internal behavior of the operating system, existing services, or third-party tools.

### 2.2 Project-Local Temporary Directory

All controllable temporary files, intermediate results, and rebuildable caches belong under:

`PROJECT_ROOT/tmp/`

Recommended layout:

```text
project/
├── AGENTS.md
├── rules/
│   └── KEEPTIDY.md
├── .gitignore
├── src/
├── tests/
├── docs/
└── tmp/
    ├── runs/
    │   └── <task-id>/
    │       ├── scratch/
    │       ├── logs/
    │       ├── outputs/
    │       ├── backups/
    │       └── state/
    ├── cache/
    ├── state/
    ├── worktrees/
    ├── bin/
    └── s/
```

Directory purposes:

| Directory | Purpose | Lifecycle |
| --- | --- | --- |
| `runs/<id>/scratch/` | Temporary scripts, test files, and intermediate data | Clean up when the task ends |
| `runs/<id>/logs/` | Debugging and test logs | Clean up after review |
| `runs/<id>/outputs/` | Screenshots, test reports, and temporary outputs | Clean up after acceptance or migration |
| `runs/<id>/backups/` | Temporary recovery copies | Clean up when recovery is no longer needed |
| `runs/<id>/state/` | Current task state and resource ownership records | Process at task completion |
| `cache/` | Reusable build, download, and tool caches | Reclaim based on size and validity |
| `state/` | Databases and service state shared across tasks | Protected; reclaim only after verification |
| `worktrees/` | Agent Git worktrees | Manage safely through Git |
| `bin/` | Temporarily compiled executables | Clean up when no longer in use |
| `s/` | Short-path resources such as Unix sockets | Clean up when the task ends |

Execution requirements:

- **MUST** create directories according to actual task needs.
- **MUST NOT** pre-create every empty directory merely to match the layout.
- **MUST** assign unique task identifiers to concurrent tasks.
- **MUST** give each Subagent a separate task directory or subdirectory.
- **MUST** prevent tasks from accidentally overwriting one another's files.
- **MUST** treat existing `tmp/` contents conservatively; establish ownership before reorganizing them.
- **SHOULD** prefer these standard directories and avoid multiple top-level temporary directories serving the same purpose.
- **SHOULD** keep task directory names short and recognizable.
- **SHOULD** retain existing appropriate permanent build directories to preserve effective incremental builds.

When multiple Agents share caches concurrently, respect the tools' own concurrency-safety requirements.

### 2.3 Git Ignore Rules

The `.gitignore` at the root of a Git project **MUST** ignore the project-local `tmp/` directory. For example:

```gitignore
# keeprules: temporary directory
/tmp/
```

- **MUST** add the rule automatically when necessary.
- **MUST** preserve existing `.gitignore` content and valid rules.
- **MUST** verify that the ignore rule actually takes effect.
- **MUST NOT** assume `.gitignore` automatically removes already-tracked files from version control.
- If tracked temporary files are found, **MUST** inspect their purpose before handling them under appropriate authorization.
- **MUST NOT** commit credentials, temporary logs, caches, or unrelated build artifacts.

### 2.4 Temporary-Directory Environment Variables

For controllable subprocesses launched by an Agent, **MUST** configure temporary directories as supported by the platform and tools.

Linux/macOS Bash example:

```bash
TASK_TMP="$PROJECT_ROOT/tmp/runs/$TASK_ID/scratch"

mkdir -p -- "$TASK_TMP"

TMPDIR="$TASK_TMP" \
TMP="$TASK_TMP" \
TEMP="$TASK_TMP" \
<command>
```

Requirements:

- `PROJECT_ROOT` must be a verified absolute path.
- `TASK_ID` must be unique among concurrently running tasks.
- The target directory must exist before launching the command.
- **MUST** use absolute temporary-directory paths wherever possible.
- **MUST** set these variables only for the current task and its subprocesses.
- **MUST NOT** modify the user's global shell configuration to do so.
- **SHOULD** configure tool-specific caches, build directories, and download locations as supported.
- **MUST NOT** assume `TMPDIR` controls every output path of every tool.

On Windows, use the corresponding process-level environment variables and platform-native path APIs.

### 2.5 Unix Sockets and Short Paths

Some Linux/macOS tools impose strict limits on Unix socket path lengths.

- **MUST** verify that the complete absolute pathname meets platform limits before creating a pathname Unix socket.
- **SHOULD** shorten task IDs, socket filenames, and intermediate directories first.
- **SHOULD** use `tmp/s/<task-id>/` when a short path is needed.
- **MUST NOT** relocate sockets to the system `/tmp/` to work around path-length limits.
- **MUST NOT** use symbolic links outside the repository to bypass authorization boundaries.
- If the project root is too long to satisfy these limits, **MUST** report the constrained operation and select a safe compatible solution.

## 3. Cache and Build Artifact Management

### 3.1 Unified Cache Principles

The same tool within a project should preferably reuse one consistent cache location.

- **MUST** check for an existing compatible, reusable cache before creating a new persistent cache.
- **SHOULD** place configurable project-level caches under `tmp/cache/`.
- **SHOULD** keep cache configuration consistent within a toolchain to reduce repeated downloads and recompilation.
- **MUST NOT** repeatedly create separate cache directories for the same tool without justification.
- **MUST NOT** break a functioning build environment merely to standardize directory names.
- **MUST** respect each tool's cache format, locking, and concurrency-safety requirements.
- **SHOULD** preserve effective incremental build caches whenever possible.
- **SHOULD** periodically inspect caches for abnormal growth and long-unused data.

When duplicate historical cache directories are found:

1. Identify the cache configuration actually in use.
2. Inspect directory sizes, contents, and active references.
3. Determine one consistent location for future use.
4. Migrate or reclaim old caches only after verifying that it is safe.

**MUST NOT** bulk-delete caches without understanding their purpose and current use.

### 3.2 Common Tool Cache Conventions

The following locations are examples; use only those relevant to the project's technology stack:

| Tool | Recommended path | Typical configuration |
| --- | --- | --- |
| Go Build | `tmp/cache/go-build/` | `GOCACHE` |
| Go Modules | `tmp/cache/go-mod/` | `GOMODCACHE` |
| Go Workspace | `tmp/cache/go-path/` | `GOPATH` |
| npm | `tmp/cache/npm/` | `npm_config_cache` |
| pnpm Store | `tmp/cache/pnpm-store/` | `store-dir` |
| Node compile cache | `tmp/cache/node-compile/` | `NODE_COMPILE_CACHE` |
| Playwright browsers | `tmp/cache/playwright-browsers/` | `PLAYWRIGHT_BROWSERS_PATH` |

Manage Playwright test output, browser profiles, and transform caches separately using their supported configuration mechanisms.

These paths are suggested conventions; the specific tool version, existing settings, and project requirements must inform the choice.

**SHOULD** reuse existing compatible caches rather than migrate them solely because their directory names differ.

For example, hash-based subdirectories created automatically inside the Go build cache are normal and need no individual reorganization.

For Rust `target/`, the official output directories of Go projects, and other existing build directories, **SHOULD** preserve project conventions.

### 3.3 Cache Cleanup

- **MUST** distinguish current-task caches, shared caches, and external tool caches.
- **MUST** verify that no other task is using a shared cache before cleaning it up.
- **MUST NOT** clean global caches in the user's home directory without authorization.
- **MUST NOT** use global cache-cleaning commands that affect other projects.
- **SHOULD** use the tool's official cache inspection and cleanup mechanisms.
- **SHOULD** prioritize reclaiming confirmed unused duplicate caches and expired download data.
- **MUST** preserve the project's actively used build and runtime capabilities after cleanup, or explicitly document what will need to be rebuilt.

## 4. Separating Temporary Artifacts from Permanent Deliverables

Classify project data as follows:

| Category | Examples | Handling |
| --- | --- | --- |
| One-off temporary files | Debugging scripts, intermediate test files | Clean up when the task ends |
| Reusable caches | Compiler caches, dependency download caches | Retain or reclaim as needed |
| Runtime state | Databases, service state, sockets | Protected; clean up only after verification |
| Permanent engineering deliverables | Specs, tickets, ADRs, design documents | Store persistently |
| Verification evidence | E2E reports, key screenshots, test records | Manage according to acceptance and retention policies |

### 4.1 Permanent Engineering Deliverables

- **MUST** store requirements, specifications, architecture decisions, and formal design documents that need long-term retention in project-approved persistent locations.
- **MUST** prefer an existing `docs/` directory, issue tracker, or other established documentation system.
- **MUST NOT** use Git-ignored `tmp/` as the sole long-term storage location for permanent engineering deliverables.
- **SHOULD** keep unapproved working drafts in `tmp/runs/<task-id>/outputs/` during development.
- **MUST** move drafts that need to be preserved into a persistent location before final delivery.
- **MUST** verify that the permanent version is complete before deleting a temporary copy.
- **MUST NOT** assume that a document has been synchronized merely because a file with the same name exists.

If specs or tickets have been completely saved as GitHub Issues, their local intermediate copies may be treated as temporary resources and safely cleaned up after verifying consistency.

### 4.2 Logs and Test Evidence

- **MUST** place controllable logs and intermediate test artifacts in the corresponding task directory.
- **SHOULD** use an organized directory structure rather than scatter logs and screenshots across the `tmp/` root.
- **MUST** retain the key error information needed to reproduce failures.
- **MUST** distinguish formal test evidence from disposable debugging output at delivery.
- **SHOULD** clean up excess duplicate logs according to task acceptance status.
- **SHOULD** rotate, size-limit, or safely truncate large logs.
- **MUST NOT** claim tests passed by deleting failure evidence, hiding errors, or fabricating reports.

### 4.3 Temporary Executables

- **SHOULD** place one-off compiled executables under `tmp/bin/` or the task's output directory.
- **MUST** check whether a process is still using an executable before deleting it.
- **MUST NOT** delete files required by currently running services.
- **SHOULD** clean up unused artifacts after task completion once their rebuildability has been confirmed.

## 5. Git and Working Tree Safety

- **MUST** inspect current Git status and relevant files before modifying them.
- **MUST** preserve uncommitted changes made by the user or other Agents.
- **MUST** inspect the final Git diff before committing or delivering work.
- **MUST NOT** run `git reset --hard`, `git clean -fdx`, force-push, rewrite history, or delete branches without authorization.
- **MUST NOT** automatically commit code without explicit authorization.
- **MUST NOT** overwrite, discard, or stash the user's existing changes without authorization.
- **MUST NOT** delete other Agents' work to resolve conflicts.
- **MUST** take recovery measures proportionate to the risk of high-impact changes.

Before bulk deletion, irreversible migration, or other significant changes, **MUST** confirm a recovery plan.

`tmp/runs/<task-id>/backups/` may be used for short-term recovery; backups requiring long-term retention must be moved to a project-approved location.

Production environments and irreversible data operations are also governed by `KEEPSECURED.md`.

## 6. Git Worktree Lifecycle

Git worktrees are protected development workspaces and may contain independent branches, uncommitted changes, and code produced by Subagents.

### 6.1 Creation and Use

- **SHOULD** manage temporary Agent worktrees together under `tmp/worktrees/`.
- **MUST** check for an existing suitable worktree that can be reused before creating another.
- **MUST** assign clear task ownership to each worktree.
- **MUST** prevent multiple Agents from concurrently modifying the same files in the same workspace.
- **MUST** respect Git's worktree management mechanisms.
- **MUST NOT** casually delete a worktree as though it were an ordinary temporary directory.

If the project already has an explicit worktree layout convention, that layout may be retained.

### 6.2 Completion and Reclamation

Before removing a worktree, **MUST**:

1. Verify that the worktree belongs to the current task and determine its lifecycle status.
2. Inspect Git's registered worktree information.
3. Check uncommitted changes and untracked files.
4. Check for commits on the branch that have not yet been integrated.
5. Check whether any Agent, shell, service, or other process is still using the directory.
6. Confirm that all work worth keeping has been merged, committed, or safely transferred.

Recommended command:

```bash
git worktree list --porcelain
```

Once cleanup conditions have been verified, use Git's native worktree management command:

```bash
git worktree remove <worktree-path>
```

- **MUST NOT** substitute recursive directory deletion for normal worktree removal.
- **MUST NOT** forcibly remove a worktree with uncommitted changes without authorization.
- **MUST NOT** delete branches containing unintegrated work without authorization.
- **MUST NOT** use the worktree's age as the sole criterion for cleanup.
- **MUST** preserve worktrees of unknown provenance, in active use, or containing unsaved work.
- **SHOULD** reclaim task-owned worktrees after task completion once safety has been verified.

If only stale worktree registration records need attention, use Git's native inspection and repair mechanisms after confirming that no workspace still needs recovery.

## 7. Database and Runtime State Protection

The project's temporary directory may contain runtime state for PostgreSQL, SQLite, Redis, or other test services.

Example:

```text
tmp/
└── state/
    └── postgres/
        ├── data/
        ├── logs/
        └── run/
```

- **MUST** treat database data directories as protected runtime state.
- **MUST** verify that the relevant service has been shut down safely before cleanup.
- **MUST** check whether the data contains test results, migration state, or user data that still need to be retained.
- **MUST** confirm that the environment may be rebuilt before resetting or cleaning it up.
- **MUST NOT** directly delete a database data directory while the database is running.
- **MUST NOT** delete database files of unverified recoverability merely to free disk space.
- **SHOULD** manage database data, logs, and sockets separately.
- **SHOULD** establish an explicit initialization and cleanup process for reproducible test databases.
- **MUST** preserve the ability to reproduce necessary state after cleanup, or clearly document how to rebuild it.

Apply the same ownership and lifecycle principles to other service state, test container volumes, and local mock services.

## 8. Processes, Containers, and Concurrency

### 8.1 Process Management

- **MUST** record the background processes created by the current task and their purposes.
- **MUST** define the lifecycle of background resources.
- **MUST** prefer to reuse existing suitable development services.
- **MUST** provide timeouts, cancellation, or status-check mechanisms for long-running operations.
- **MUST** stop task-created background resources that no longer need to be retained when the task ends.
- **MUST NOT** arbitrarily terminate processes belonging to users, the system, or other tasks.
- **MUST NOT** infer process ownership from PID alone; use startup information or other verifiable identifiers when cleaning up.
- **SHOULD** use task identifiers, process groups, or other appropriate means to manage background processes.

### 8.2 Docker and Containers

- **MUST** maintain recognizable task ownership for temporary containers and related resources.
- **MUST** check whether containers, volumes, and networks are still in use before cleanup.
- **MUST NOT** perform global Docker cleanup without authorization.
- **MUST NOT** remove container resources created by other tasks, projects, or users.
- **SHOULD** use container labels, project names, or similar mechanisms to distinguish ownership.
- **SHOULD** reclaim temporary task-owned containers that are no longer needed at task completion.

### 8.3 Subagents and Concurrency

- **MUST** ensure that all Subagents inherit the applicable engineering rules.
- **MUST** define clear file, worktree, and resource boundaries for concurrent tasks.
- **MUST** assign separate temporary directories to concurrent tasks.
- **MUST** check conflicts and verification status before integrating results.
- **MUST NOT** delegate recursively without bounds or allow unbounded concurrency.
- **SHOULD** adjust concurrency according to CPU, memory, disk capacity, and project requirements.
- **SHOULD** avoid having multiple Subagents repeat expensive builds and tests unnecessarily.

These constraints do not require additional mandatory handshakes, dedicated daemons, or complex management frameworks.

## 9. Disk, Log, and Resource Budgets

- **MUST** monitor the overall size of project `tmp/`, file counts, and abnormal growth.
- **MUST** consider remaining disk space during large builds, tests, downloads, and concurrent tasks.
- **MUST** prevent logs, caches, and test artifacts from growing without bound.
- **MUST** pause new resource consumption and diagnose the situation when an explicit resource budget is reached.
- **MUST** inspect resources generated by the current task first.
- **MUST NOT** expand cleanup permissions because disk space is low.
- **SHOULD** establish reasonable soft budgets appropriate to project scale.
- **SHOULD** check resource consumption periodically during long-running tasks.
- **SHOULD** place reasonable size limits on log output.
- **SHOULD** avoid unnecessary repeated downloads, builds, and test artifact generation.

Initial soft reference values:

| Resource | Suggested value |
| --- | --- |
| Temporary artifacts per task | 2 GiB |
| Concurrent Subagents | 4 |
| Review period for completed-task logs | 7 days |

These values are adjustable reference thresholds. Large projects and build tasks may require higher budgets; reaching a resource threshold does not itself grant permission to delete anything.

### 9.1 File Watching and Scanning

- **SHOULD** exclude `tmp/` from unrelated file watching, IDE indexing, and recursive test discovery as appropriate to the project.
- **SHOULD** avoid repeatedly scanning large cache directories during builds.
- **SHOULD** explicitly define how temporary directories are handled in project backup and packaging workflows.
- **MUST NOT** make broad, unauthorized changes to existing build configuration solely to exclude temporary directories.
- **MUST** verify that changes to file watching, indexing, and build configuration do not affect real source files.

## 10. Error Handling and Retries

- **MUST** inspect the real exit status and key error messages after a failure.
- **MUST** determine whether a failure concerns code, dependencies, environment, permissions, networking, or external services.
- **MUST** address root causes first for deterministic failures.
- **MUST** apply limited retries and appropriate backoff for transient failures.
- **MUST** change diagnostic strategy after two consecutive failures under the same conditions.
- **MUST** inspect existing state before retrying operations with side effects.
- **MUST NOT** repeat the same failing command indefinitely.
- **MUST NOT** claim success by hiding errors, skipping checks, or fabricating results.
- **SHOULD** retain necessary error evidence.
- **SHOULD** continue independent, authorized work when external blockers are encountered.

Logs, scripts, and temporary patches created during debugging must follow the project-local temporary-directory conventions.

## 11. Temporary Resource Lifecycle and Safe Reclamation

### 11.1 Resource Creation

Each controllable temporary resource should have an identifiable:

- **Owner**: The task or Agent that created it.
- **Purpose**: Why the resource is needed.
- **Lifecycle**: When it may stop being used.
- **Cleanup method**: How it can be safely reclaimed.
- **Retention reason**: Whether there are dependencies or results awaiting review.

For ordinary short tasks, the directory name and existing execution context can establish ownership; complex tasks may record necessary state within the task directory.

### 11.2 Cleanup After Normal Completion

When a task finishes normally, **MUST**:

1. Inspect temporary files created by the current task.
2. Remove confirmed unused files under `scratch/` and short-path resources.
3. Handle the task's background processes and temporary services.
4. Review logs, screenshots, test reports, and temporary backups.
5. Move results requiring long-term retention to permanent locations.
6. Determine whether worktrees used by the task need to be retained.
7. Preserve effective shared caches that remain in use.
8. Accurately report resources that could not be cleaned up or must be retained.

**MUST NOT** unconditionally recursively delete the entire `tmp/` directory.

### 11.3 Historical Resource Reclamation

For directories left by interrupted tasks, old tasks, or unknown sources, **MUST** perform safety checks first.

Before cleanup, verify that:

- The resource is within the current project's authorized scope.
- Its real path does not cross boundaries or escape through symbolic links.
- Its ownership can be reliably established.
- The corresponding task has finished.
- No active process or other Agent is using it.
- It contains no uncommitted, unintegrated, or unsaved work.
- It contains no important test evidence, database state, or recovery copies.
- The user has not explicitly requested that it be retained.

- **SHOULD** list candidate resources and estimated recoverable space first.
- **SHOULD** use a dry run or equivalent preview when possible.
- **MUST** clean up only resources confirmed safe and within the currently authorized scope.
- **MUST NOT** decide that something is safe to delete based only on directory age, task number, or disk usage.
- **MUST NOT** follow symbolic links during recursive deletion.
- **MUST NOT** remove resources owned by other tasks, users, or the system.
- **MUST NOT** expand the deletion scope when safety cannot be established.

Whether historical resources may be reclaimed depends on the results of these checks. Expiration of a retention period triggers review; it does not automatically authorize deletion.

### 11.4 Automatic Organization and Reclamation

- **MUST** perform appropriate cleanup of the current task's resources at each task completion.
- **SHOULD** check for obvious resource anomalies at the start of new tasks.
- **SHOULD** automatically reclaim resources belonging to the current task that are confirmed finished, have clear ownership, and have no retention requirement.
- **SHOULD** retain effective caches that materially improve future build efficiency.
- **MUST** verify target paths and resource ownership before automatic cleanup.
- **MUST** record the reason and preserve the existing state if cleanup fails.
- **MUST NOT** unconditionally delete unknown historical directories on a schedule.
- **MUST NOT** forcibly stop other tasks to complete cleanup.

For long-running projects, prefer lightweight end-of-task cleanup and on-demand inspections.

## 12. Dependencies and Development Environment

- **MUST** prefer the project's existing package managers, lockfiles, and development toolchains.
- **MUST** check for existing equivalent capabilities before adding dependencies.
- **MUST** prefer project-local or isolated environments for development dependencies.
- **MUST NOT** upgrade or modify global toolchains without authorization.
- **MUST NOT** delete lockfiles to work around ordinary dependency conflicts.
- **MUST NOT** make unrelated, large-scale environment changes to solve a local problem.
- **SHOULD** manage configurable download caches and intermediate build artifacts consistently.
- **SHOULD** avoid reinstalling the same dependencies across tasks unnecessarily.
- **MUST** retain enough dependency error information to make failures diagnosable.

If the project already has stable cache and dependency management practices, prioritize compatibility with them.

## 13. Testing, Verification, and Engineering Quality

- **MUST** follow the project's existing architecture, interfaces, and coding standards.
- **MUST** run appropriate formatting checks, static checks, and tests according to the actual changes.
- **MUST** accurately record test commands and execution results.
- **MUST** distinguish between implemented code, executed tests, and verified functionality.
- **MUST NOT** conceal problems by skipping, deleting, or weakening tests.
- **MUST NOT** use fabricated data or false success results as acceptance evidence.
- **SHOULD** cover normal paths, failure paths, and important edge cases.
- **SHOULD** test the affected scope first, then broaden verification according to risk.
- **SHOULD** avoid expensive repeated tests unrelated to the current changes.
- **MUST** place controllable temporary artifacts generated by tests under the project's temporary directory.

Long-running E2E tests should have reasonable timeouts, resource checks, and mechanisms for retaining failure evidence.

## 14. Delivery Checklist

Before completing the task, **MUST** verify:

- [ ] Work remained within the currently authorized task scope.
- [ ] Existing user files and uncommitted changes were preserved.
- [ ] Controllable temporary files were kept under project `tmp/`.
- [ ] System `/tmp/` was not actively accessed or cleaned up.
- [ ] `.gitignore` correctly ignores project `/tmp/`.
- [ ] There are no unnecessary duplicate cache directories.
- [ ] Logs and intermediate artifacts for the task were organized appropriately.
- [ ] Final specs, tickets, and documents were saved to persistent locations.
- [ ] Worktree cleanup caused no loss of work.
- [ ] Database and service runtime state was preserved.
- [ ] Background resources created by this task were handled properly.
- [ ] Unneeded task-specific temporary files were safely cleaned up.
- [ ] Required tests and checks were actually executed.
- [ ] Git status and the final change scope were inspected.
- [ ] Remaining issues, external blockers, and retained resources were clearly reported.

The final delivery summary should briefly include:

1. **Changes completed**: What was actually done.
2. **Verification results**: Which tests were run and their outcomes.
3. **Remaining issues**: What remains incomplete or requires attention.
4. **Resource status**: How temporary files, caches, services, and worktrees were handled.

For simple tasks, combine these items rather than writing an unnecessarily long report.

## 15. Rule Applicability and Conflicts

- System, platform, and other higher-priority constraints always apply.
- Explicit user authorization defines the current task scope; these rules do not expand permissions.
- Security and sensitive-information protection follow `KEEPSECURED.md`.
- Implementation complexity, necessity, and the smallest maintainable changes follow `KEEPLEAN.md`.
- Preserve and respect reasonable existing project conventions.
- When rules conflict, **MUST** choose a safe, reversible, and narrowly scoped approach first.
- **MUST** report conflicts that cannot be resolved safely.

**All Coding Agents and Subagents MUST follow these rules throughout task creation, execution, testing, cleanup, and delivery.**

The goal is a consistently tidy engineering environment where temporary resources have clear ownership, predictable lifecycles, and safe reclamation procedures while development remains efficient, straightforward, and maintainable.

## Summary for `AGENTS.md`

The `agents` step injects the marked region below into the project's `AGENTS.md`, so the habits that keep a workspace safe — where temporary files live, what must not be touched, what must be reclaimed — travel with the reference instead of requiring the whole rule. Keep it behavioral and short.

<!-- keeprules:agents -->
### Engineering hygiene

- Never actively access or clean the system `/tmp/` directory.
- Store controllable temporary artifacts under the project `tmp/` and ensure it is Git-ignored.
- Isolate task resources and reuse compatible project caches.
- Protect user changes, active services, databases, and Git Worktrees.
- Clean up only verified task-owned resources that are no longer needed.
<!-- /keeprules:agents -->
