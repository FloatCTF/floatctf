import dayjs from "dayjs";
import utc from "dayjs/plugin/utc";
import { useState } from "react";
dayjs.extend(utc);

/**
 * 专门用于在 UI 上展示绝对时间的格式化函数
 * @param datetime 后端传来的 ISO 8601 字符串 (含时区)
 * @param fallback 为空时的占位符，默认显示 "-"
 */
export function DatetimeToShow(
    datetime: string | undefined | null,
    fallback = "-",
) {
    if (!datetime) {
        return fallback; // 安全兜底
    }

    // dayjs 只要吃进带 Z 或时区偏移的字符串，默认就会吐出本地时间，极其聪明
    return dayjs(datetime).format("YYYY-MM-DD HH:mm:ss");
}

// biome-ignore lint/suspicious/noExplicitAny: <explanation>
export const diffToPatch = <T extends Record<string, any>>(
    original: T,
    current: T,
    options?: {
        allowKeys?: (keyof T)[];
        ignoreKeys?: (keyof T)[];
        keepId?: boolean; // 默认智能控制
    },
) => {
    const { allowKeys, ignoreKeys = [], keepId = true } = options ?? {};

    // 确定实际要比较的字段集
    const baseKeys =
        allowKeys && allowKeys.length > 0
            ? allowKeys
            : (Object.keys(current) as (keyof T)[]);

    // ⚙️ 智能判断是否保留 id
    const shouldKeepId =
        // 1️⃣ 明确在 allowKeys 里 → 保留
        allowKeys?.includes("id" as keyof T) ||
        // 2️⃣ 没传 allowKeys、没显式 ignore id 且 keepId=true → 保留
        (keepId && !ignoreKeys.includes("id" as keyof T));

    const result: Partial<T> = {};

    // ✅ 保留 id（如果符合规则）
    if (shouldKeepId && "id" in current && current.id !== undefined) {
        // @ts-ignore
        result.id = current.id;
    }

    for (const key of baseKeys) {
        if (ignoreKeys.includes(key)) continue;
        if (key === "id") continue; // 防止重复处理 id
        if (current[key] !== original[key] && current[key] !== undefined) {
            result[key] = current[key];
        } else if (current[key] !== original[key]) {
            console.log(`diffToPatch: ${String(key)} changed`, {
                original: original[key],
                current: current[key],
                resultKey: result[key],
            });
        }
        // 如果值没变化，不设置 null，让后端保持原值
    }
    console.log("diffToPatch result:", result);

    return result;
};

export const useSelectedRowIds = () => {
    return useState<Set<string>>(new Set());
};
