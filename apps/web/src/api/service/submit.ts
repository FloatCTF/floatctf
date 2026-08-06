import type { ChallengeSolves } from "@/entity";
import { type UniResponse, service_api } from "../axios";

export const submitServiceApi = {
    submit: async ({
        instance_id,
        flag,
    }: {
        instance_id: string;
        flag: string;
    }): Promise<UniResponse<ChallengeSolves>> => {
        const res = await service_api.post("/submit/flag", {
            instance_id,
            flag,
        });
        return res.data;
    },
    // 	#[derive(Debug, MultipartForm)]
    // pub struct WriteupForm {
    //     #[multipart(limit = "1024MB")]
    //     writeup_pdf: TempFile,
    //     event_id: Text<Uuid>,
    //     team_id: Option<Text<Uuid>>,
    // }
    submitWriteup: async (
        file: File,
        event_id: string,
        team_id?: string,
    ): Promise<UniResponse<null>> => {
        const formData = new FormData();
        formData.append("writeup_pdf", file);
        formData.append("event_id", event_id);
        if (team_id) {
            formData.append("team_id", team_id);
        }
        const res = await service_api.post("/submit/writeup", formData, {
            headers: {
                "Content-Type": "multipart/form-data",
            },
        });
        return res.data;
    },
    submitSingle: async ({
        event_id,
        instance_id,
        flag,
    }: {
        event_id: string;
        instance_id: string;
        flag: string;
    }): Promise<UniResponse<ChallengeSolves>> => {
        const res = await service_api.post("/submit/flag", {
            event_id,
            instance_id,
            flag,
        });
        return res.data;
    },
};
