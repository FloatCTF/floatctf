# Issue tracker：GitHub

Issue 与 spec 都记在本仓库的 GitHub Issues（`FloatCTF/floatctf`），统一用 `gh`。

## 常用操作

| 目的 | 命令 |
|---|---|
| 新建 | `gh issue create --title "…" --body-file -`（多行用 heredoc） |
| 查看 | `gh issue view <n> --json number,title,body,labels,comments` |
| 列表 | `gh issue list --state open --json number,title,labels` |
| 评论 | `gh issue comment <n> --body "…"` |
| 标签 | `gh issue edit <n> --add-label …` / `--remove-label …` |
| 关闭 | 先 `gh issue comment`，再 `gh issue close <n>` |

- 仓库由 `git remote -v` 推断；issue 只进 `FloatCTF/floatctf`。
- 子任务：`gh issue create --parent <n>`；旧版 gh 在子 issue 顶部写 `Part of #<n>`。
- 依赖：`gh issue edit <child> --add-blocked-by <blocker>`；旧版 gh 写 `Blocked by: #<n>`。

## PR 是否进入 triage

**否。** 将来若把外部 PR 当需求入口，把这里改成「是」，`/triage` 会读这一行。

## 术语约定

- 「发布到 issue tracker」= 建 GitHub issue。
- 「取回相关 ticket」= 按上表查看。
