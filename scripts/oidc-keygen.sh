#!/usr/bin/env bash
# 生成一把 FloatCTF OIDC 签名私钥（RSA 3072，PKCS#8 PEM，权限 0600）。
#
# 用法：scripts/oidc-keygen.sh <输出目录> [kid]
#   kid 缺省为 "oidc-<UTC 日期>-<随机 4 hex>"；文件名为 <kid>.pem。
#
# 生成后在 TOML 中引用（详见 docs/identity/oidc-operations.md）：
#   [[oidc.signing_keys]]
#   kid = "<kid>"
#   private_key_file = "<输出目录>/<kid>.pem"
#   active = true
#
# 私钥只在本机生成、不打印到终端；不要提交到 Git，不要放进数据库。
set -Eeuo pipefail

out_dir="${1:?用法: $0 <输出目录> [kid]}"
kid="${2:-oidc-$(date -u +%Y%m%d)-$(openssl rand -hex 2)}"

if [[ ! "$kid" =~ ^[A-Za-z0-9._-]{1,64}$ ]]; then
    echo "kid 非法：只允许 [A-Za-z0-9._-]，1-64 字符" >&2
    exit 1
fi
command -v openssl >/dev/null || { echo "需要 openssl" >&2; exit 1; }

mkdir -p "$out_dir"
chmod 700 "$out_dir" 2>/dev/null || true
target="$out_dir/$kid.pem"
if [[ -e "$target" ]]; then
    echo "拒绝覆盖已存在的 key：$target" >&2
    exit 1
fi

umask 077
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 -out "$target" 2>/dev/null
chmod 600 "$target"

echo "kid=$kid"
echo "private_key_file=$target"
echo "public_key_sha256=$(openssl pkey -in "$target" -pubout -outform DER 2>/dev/null | openssl dgst -sha256 -r | cut -d' ' -f1)"
