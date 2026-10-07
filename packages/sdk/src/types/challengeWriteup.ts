import type { ChallengeWriteup, Challenges } from "../entity/index.js";

/** 题目 Writeup 详情（含作者与所属题目）。 */
export type ChallengeWriteupResult = {
	id: string;
	nickname: string;
	avatar?: string;
	email: string;
	challenge: Challenges;
	writeup: ChallengeWriteup;
};
