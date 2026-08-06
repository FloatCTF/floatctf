#!/usr/bin/env bash

# 必须使用 source 执行，才能把变量导出到当前 Shell
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    echo "请使用：source ./activate.sh"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export PROJECT_ROOT="$SCRIPT_DIR"
export PATH="$PROJECT_ROOT/scripts:$PATH"

echo "FloatCTF environment activated"
echo "PROJECT_ROOT=$PROJECT_ROOT"
