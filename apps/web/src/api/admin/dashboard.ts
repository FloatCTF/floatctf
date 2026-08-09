import { type UniResponse, admin_api } from "@/api/axios";

export type DashboardSummary = {
	stats: {
		users: number;
		events: number;
		challenges: number;
		weapons: number;
		announcements: number;
		discussions: number;
		instances: number;
		gameboxes: number;
	};
	attention: {
		failed_tasks: Array<{
			task_name: string;
			task_key: string;
			error_msg: string | null;
			attempt_count: number;
			max_attempts: number;
			updated_at: string;
		}>;
		error_logs_24h: number;
		awd_alerts: Array<{
			event_id: string;
			title: string;
			status: string;
			phase: string;
		}>;
	};
	events: Array<{
		event_id: string;
		title: string;
		event_type: string;
		start_time: string;
		end_time: string;
		hidden: boolean;
		awd: {
			status: string;
			phase: string;
			started_at: string | null;
		} | null;
	}>;
	activity: {
		recent_solves: Array<{
			nickname: string;
			avatar: string | null;
			challenge_name: string;
			solved_at: string;
		}>;
		recent_signups: Array<{
			nickname: string;
			username: string;
			avatar: string | null;
			created_at: string;
		}>;
	};
};

export const dashboardAdminApi = {
	/** GET /api/admin/dashboard/summary —— 一次拿到总览所需全部聚合数据 */
	summary: async (): Promise<UniResponse<DashboardSummary>> => {
		const res = await admin_api.get("/dashboard/summary");
		return res.data;
	},
};
