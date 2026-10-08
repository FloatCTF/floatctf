/** 未匹配路由的兜底页：可见、可恢复，不是白屏。 */

import { Link } from "react-router";

import { PageBody } from "~/components/app/page";
import { NotFoundBlock } from "~/components/app/states";
import { Button } from "~/components/ui/button";

export function NotFoundPage() {
	return (
		<PageBody className="max-w-2xl">
			<NotFoundBlock
				title="页面不存在"
				description="这个地址没有对应的页面。可能是链接过期，或该资源已被删除。"
				action={
					<div className="flex flex-wrap items-center justify-center gap-2">
						<Button asChild>
							<Link to="/">返回选手端</Link>
						</Button>
						<Button variant="outline" asChild>
							<Link to="/admin">前往管理端</Link>
						</Button>
					</div>
				}
			/>
		</PageBody>
	);
}
