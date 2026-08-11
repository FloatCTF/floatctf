#!/usr/bin/env python3
"""GameBox 批量健康检查（judge）。

Trusted by the platform; never baked into the Docker image.
支持一次传入多个 GameBox IP 并发检查。

用法:
    python judge/check.py ip1 ip2 ip3 ip4 ip5 ip6 ...

输出（JSON，stdout）:
    [{"success": true,  "gamebox_ip": "ip1"},
     {"success": false, "gamebox_ip": "ip2", "error": "..."},
     ...]

退出码: 全部成功 → 0；任一失败 → 1；用法错误（未传 IP）→ 2
"""
import json
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor

# 单次检查超时（秒）
CHECK_TIMEOUT_SECS = 10
# 并发检查上限
MAX_WORKERS = 32


def check(target_ip: str) -> dict:
    """检查单个 GameBox 的 HTTP 服务是否正常。"""
    result: dict = {"gamebox_ip": target_ip}
    try:
        with urllib.request.urlopen(
            f"http://{target_ip}/", timeout=CHECK_TIMEOUT_SECS
        ) as resp:
            if resp.status == 200:
                result["success"] = True
            else:
                result["success"] = False
                result["error"] = f"HTTP {resp.status}"
    except Exception as exc:  # 网络/超时/HTTP 错误
        result["success"] = False
        result["error"] = str(exc)
    return result


def main() -> int:
    ips = sys.argv[1:]
    if not ips:
        print(
            json.dumps(
                {"success": False, "error": "用法: python judge/check.py ip1 ip2 ip3 ..."}
            )
        )
        return 2

    with ThreadPoolExecutor(max_workers=min(MAX_WORKERS, len(ips))) as pool:
        results = list(pool.map(check, ips))

    print(json.dumps(results, ensure_ascii=False))
    return 0 if all(r.get("success") for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
