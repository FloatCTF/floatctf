import { Avatar, Spinner, Truncate } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { Link, createFileRoute } from "@tanstack/react-router";

import { serviceApi } from "@/api";
import type { ChallengeWriteup, Challenges } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/challenges/$id/writeup")({
    component: RouteComponent,
});

export type ChallengeWriteupResult = {
    id: string;
    nickname: string;
    avatar?: string;
    email: string;
    challenge: Challenges;
    writeup: ChallengeWriteup;
};
function RouteComponent() {
    const { id } = Route.useParams();
    const { data, isLoading, isError } = useQuery({
        queryKey: ["challenge-writeup", id],
        queryFn: () => serviceApi.challenges.getWriteups(id),
    });

    if (isLoading) {
        return <Spinner />;
    }

    if (isError) {
        return <div className="text-red-500">加载失败，请稍后重试</div>;
    }

    const raw = data?.data;
    const writeups: ChallengeWriteupResult[] = Array.isArray(raw) ? raw : [];

    if (writeups.length === 0) {
        return <div className="text-gray-500">暂无 Writeup</div>;
    }
    return (
        <div className="flex flex-col gap-2 h-full w-full overflow-y-auto">
            {writeups.map((writeup) => (
                <div
                    key={writeup.email}
                    className="feed-item-content d-flex flex-column pt-2 pb-2 border color-border-default rounded-2 color-shadow-small width-full height-fit"
                >
                    <div className="repo-card d-flex rounded p-3 position-relative">
                        <div className="d-flex flex-column flex-1">
                            <div className="d-flex flex-items-center gap-2">
                                {writeup.avatar ? (
                                    <Avatar src={writeup.avatar} size={24} />
                                ) : (
                                    <div
                                        className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium flex-shrink-0"
                                        style={{
                                            width: 24,
                                            height: 24,
                                            fontSize: 10,
                                        }}
                                    >
                                        {writeup.nickname?.[0]?.toUpperCase() ||
                                            "?"}
                                    </div>
                                )}
                                <Link
                                    to="/service/writeups/$id"
                                    params={{ id: writeup.writeup.id }}
                                >
                                    {writeup.nickname}/{writeup.challenge.name}
                                </Link>
                            </div>

                            <div className="mt-2 text-muted">
                                <Truncate
                                    title="Some example text"
                                    maxWidth="100%"
                                >
                                    {(() => {
                                        // 原始文本处理
                                        const text = writeup.writeup.content
                                            .replace(/!\[.*?\]\(.*?\)/g, "") // 去掉图片
                                            .replace(
                                                /\[([^\]]+)\]\([^\)]+\)/g,
                                                "$1",
                                            ) // 去掉链接，保留文字
                                            .replace(/(`{1,3})(.*?)\1/g, "$2") // 去掉代码块/行内代码
                                            .replace(/[*_~>#-]+/g, "") // 去掉粗体/斜体/标题/列表符号
                                            .replace(/\n+/g, "\n") // 保留换行，方便取第一行
                                            .trim();

                                        // 拿第一行，且首字符不是 <
                                        const firstLine =
                                            text
                                                .split("\n")
                                                .find(
                                                    (line) =>
                                                        line && line[0] !== "<",
                                                ) || "";

                                        return firstLine.slice(0, 50); // 截取前 50 个字符
                                    })()}
                                </Truncate>
                            </div>
                        </div>

                        {/* 右下角时间 */}
                        <div className="position-absolute bottom-2 right-3 text-xs text-muted">
                            {DatetimeToShow(writeup.writeup.created_at)}
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
}
