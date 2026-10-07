import type { Instances } from "./instanceDto.js";

/** 赛事实例列表条目（含归属信息）。 */
export type EventInstanceResult = {
	id: string;
	instance: Instances;
	challenge_name: string;
	user_nickname: string;
};
