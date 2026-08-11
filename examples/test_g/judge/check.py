#!/usr/bin/env python3
"""GameBox functional check (judge).

Trusted by the platform; never baked into the Docker image.
Exit 0 = service OK, non-zero = down / broken.

用法:
    python judge/check.py <GameBox IP>

输出（JSON，stdout）:
    {"success": true}
    {"success": false, "error": "..."}
"""
import json
import sys
import urllib.request


def check(target_ip: str) -> dict:
    try:
        with urllib.request.urlopen(f"http://{target_ip}/", timeout=10) as resp:
            if resp.status == 200:
                return {"success": True}
            return {"success": False, "error": f"HTTP {resp.status}"}
    except Exception as exc:  # 网络/超时/HTTP 错误
        return {"success": False, "error": str(exc)}


def main() -> int:
    if len(sys.argv) != 2:
        print(
            json.dumps(
                {"success": False, "error": "用法: python judge/check.py <GameBox IP>"}
            )
        )
        return 2
    result = check(sys.argv[1])
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result.get("success") else 1


if __name__ == "__main__":
    sys.exit(main())
