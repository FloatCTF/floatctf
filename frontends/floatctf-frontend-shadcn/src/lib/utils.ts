import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** shadcn/ui 标准类名合并工具。 */
export function cn(...inputs: ClassValue[]): string {
	return twMerge(clsx(inputs));
}
