#!/bin/sh
set -eu

# FloatCTF dynamic flag runtime contract:
# 将 FLAG 写入 /flag 后，必须在最终 exec 的同一个 shell 中 unset，
# 防止应用进程通过 getenv / /proc/<pid>/environ 读取真实 FLAG。
if [ -n "${FLAG:-}" ]; then
    printf '%s\n' "$FLAG" > /flag
    unset FLAG
fi

exec "$@"
