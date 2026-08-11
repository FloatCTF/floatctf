import { admin_api } from "@/api/axios";

export const downloadAdminApi = {
    download: async (key: string): Promise<void> => {
        const res = await admin_api.get(`/download`, {
            params: { key },
        });
        const url = res.data.data;
        const blobRes = await fetch(url);
        // 4xx/5xx（如 RustFS 验签失败返回的 403 XML）不保存成 .pdf/.zip
        if (!blobRes.ok) {
            throw new Error(`download failed: HTTP ${blobRes.status}`);
        }
        const blob = await blobRes.blob();
        // 内容类型校验：XML 错误页不应被当作文件保存
        if (blob.type.includes("xml") || blob.type.includes("html")) {
            throw new Error(`download failed: unexpected content type ${blob.type}`);
        }
        const blobUrl = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = blobUrl;
        a.download = key.split("/").pop() || "download";
        a.click();
        URL.revokeObjectURL(blobUrl);
    },
};
