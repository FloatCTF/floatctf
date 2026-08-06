import { type UniResponse, service_api } from "../axios";

export const uploadsServiceApi = {
    upload_image: async (image_file: File): Promise<UniResponse<string>> => {
        const formData = new FormData();
        formData.append("image_file", image_file);
        const res = await service_api.post("/uploads/image", formData, {
            headers: {
                "Content-Type": "multipart/form-data",
            },
        });
        return res.data;
    },
    upload_avatar: async (image_file: File): Promise<UniResponse<string>> => {
        const formData = new FormData();
        formData.append("image_file", image_file);
        const res = await service_api.patch("/uploads/avatar", formData, {
            headers: {
                "Content-Type": "multipart/form-data",
            },
        });
        return res.data;
    },
};
