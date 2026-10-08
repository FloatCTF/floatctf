/**
 * 全局提示（toast）—— 包一层 shadcn `sonner`，统一本前端的话术形态。
 * 任何 mutation 都必须给出成功/失败反馈；失败文案用 `errorText(error)`（后端文案优先）。
 */

import { toast as sonnerToast } from "sonner";

import { errorText } from "~/api/errors";

export const toast = {
	success(title: string, description?: string) {
		sonnerToast.success(title, description ? { description } : undefined);
	},
	error(title: string, description?: string) {
		sonnerToast.error(title, description ? { description } : undefined);
	},
	info(title: string, description?: string) {
		sonnerToast.info(title, description ? { description } : undefined);
	},
	warning(title: string, description?: string) {
		sonnerToast.warning(title, description ? { description } : undefined);
	},
	loading(title: string) {
		return sonnerToast.loading(title);
	},
	/** 统一的「操作失败」：标题 + 真实后端文案。 */
	apiError(title: string, error: unknown) {
		sonnerToast.error(title, { description: errorText(error) });
	},
	dismiss(id?: string | number) {
		sonnerToast.dismiss(id);
	},
};

export function useToast(): typeof toast {
	return toast;
}
