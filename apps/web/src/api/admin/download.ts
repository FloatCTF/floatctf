import { admin_api } from "@/api/axios";

export const downloadAdminApi = {
    download: async (key: string): Promise<void> => {
        const res = await admin_api.get(`/download`, {
            params: { key },
        });
        const url = res.data.data;
        const blobRes = await fetch(url);
        const blob = await blobRes.blob();
        const blobUrl = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = blobUrl;
        a.download = key.split("/").pop() || "download";
        a.click();
        URL.revokeObjectURL(blobUrl);
    },
};
