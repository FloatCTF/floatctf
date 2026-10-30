# Domain Docs

工程类 skill 探索代码库前读什么，以及写作用词的规则。

## 先读

- `GLOSSARY.md`（仓库根）：领域术语。
- `docs/adr/`：与本次改动相关的决策记录。

两者**当前都不存在**：不存在就静默跳过，不要提示创建；等 `/domain-modeling` 真正定下术语或决策时再懒创建。

## 布局：single-context

```
/
├── GLOSSARY.md
├── docs/adr/0001-….md
├── apps/  frontends/  packages/  crates/
```

整个平台共用一套术语，不按包拆。

## 用词

- 输出里的领域概念（issue 标题、方案、测试名）必须用 `GLOSSARY.md` 里的词，不换同义词。
- 需要的词不在表里：要么在自造词（改），要么是真缺口（记给 `/domain-modeling`）。

## ADR 冲突

与既有 ADR 冲突时显式指出，不要悄悄覆盖：

> _与 ADR-0007 冲突，但值得重开，因为…_
