import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";

export function createUploadsServiceApi(http: FloatCTFHttpClient) {
	return {
	    upload_image: async (image_file: File): Promise<UniResponse<string>> => {
	        const formData = new FormData();
	        formData.append("image_file", image_file);
	        const res = await http.post("/uploads/image", formData, {
	            headers: {
	                "Content-Type": "multipart/form-data",
	            },
	        });
	        return res.data;
	    },
	    upload_avatar: async (image_file: File): Promise<UniResponse<string>> => {
	        const formData = new FormData();
	        formData.append("image_file", image_file);
	        const res = await http.patch("/uploads/avatar", formData, {
	            headers: {
	                "Content-Type": "multipart/form-data",
	            },
	        });
	        return res.data;
	    },
};
}

export type UploadsServiceApi = ReturnType<typeof createUploadsServiceApi>;
