# OIDC 单元测试 key（仅测试用）

`test-key-a.pem`（PKCS#8）与 `test-key-b.pem`（PKCS#1）是专为 `keys.rs` / OIDC 集成测试生成的
2048 位 RSA 私钥，**公开且不安全**，只能用于测试。任何部署都不得引用这两个文件；生产 key 生成方法见
`docs/identity/oidc-operations.md`。
