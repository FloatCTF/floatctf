# Parity fixtures — 直接取自 FloatCTF/floatctf-content@main

本目录下的 fixture 是 **权威 Content Contract 的逐字节副本**，来源：

```text
https://github.com/FloatCTF/floatctf-content
commit 16812b2ada8dbde4d3ba377e528eb7c45b3ea86a  (main, "Catalog (#5)")
scripts/tests/fixtures/valid/...
scripts/tests/fixtures/safe-names/...
```

它们被 `tests/content_contract_parity.rs` 使用，用来证明：

- `floatctf-content` 的 canonical Challenge（container / static）能被 FCMC 接受；
- canonical GameBox（**没有** `[gamebox]` 段）能以 GameBox 模式被 FCMC 接受；
- `safe_name` / `version` / image ref 与 `scripts/content.py` + `catalog.json` 一致。

**不要手工修改这些文件**：它们必须与上游完全一致。需要新增 parity 用例时，
从上游重新复制，而不要就地编辑。
