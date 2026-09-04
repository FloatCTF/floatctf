#!/usr/bin/env python3
"""Trusted AWD judge script (never included in the Docker image)."""
import sys
import urllib.request

def main() -> int:
    if len(sys.argv) < 2:
        print("usage: check.py <target_ip>", file=sys.stderr)
        return 2
    target = sys.argv[1]
    try:
        with urllib.request.urlopen(f"http://{target}/", timeout=5) as resp:
            body = resp.read().decode("utf-8", errors="replace")
            if "hello" in body.lower() or resp.status == 200:
                print("OK")
                return 0
    except Exception as e:
        print(f"FAIL: {e}", file=sys.stderr)
        return 1
    print("FAIL", file=sys.stderr)
    return 1

if __name__ == "__main__":
    raise SystemExit(main())
