import { UnderlineNav } from "@primer/react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";

import { serviceApi } from "@/api";
import { MDPlusEditor, useMsgBanner } from "@/components";
import { ServiceRouteGuard } from "@/routes/service/route";
import { RouterNavItem } from "../../events/jeopardy.$id/route";

export const Route = createFileRoute("/service/challenges/$id")({
    component: RouteComponent,
    loader: ServiceRouteGuard,
});

function RouteComponent() {
    const { id } = Route.useParams();
    const [markdown, setMarkdown] = useState<string>("");
    const { data: writeup, isLoading } = useQuery({
        queryKey: ["my-challenge-writeup", id],
        queryFn: () => serviceApi.challenges.getMyWriteup(id),
    });
    const banner = useMsgBanner();
    const wpMutations = useMutation({
        mutationFn: serviceApi.challenges.createMyWriteup,
        onSuccess: () => {
            banner.showBanner("success", "Writeup saved successfully");
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    useEffect(() => {
        const data = writeup?.data;
        if (data?.content) {
            setMarkdown(data?.content);
        }
    }, [writeup]);

    return (
        <div className="flex h-full w-full flex-col">
            <UnderlineNav aria-label="Repository">
                <RouterNavItem to="/service/challenges/$id" params={{ id }}>
                    Challenge
                </RouterNavItem>
                <RouterNavItem
                    to="/service/challenges/$id/writeup"
                    params={{ id }}
                >
                    WriteUp
                </RouterNavItem>
            </UnderlineNav>

            <div className="flex h-full w-full min-h-0">
                {/* 左侧 */}
                <div
                    id="info"
                    className="flex flex-col p-2 my-2 flex-5 min-h-0"
                >
                    <Outlet />
                </div>

                {/* 右侧 */}
                <div
                    id="challenge-wp"
                    className="flex-7 h-full flex flex-col min-h-0 border-l"
                >
                    <banner.BannerComponent />
                    <MDPlusEditor
                        className="flex-1 min-h-0"
                        value={markdown}
                        setValue={(value) => setMarkdown(value)}
                        onSave={() => {
                            wpMutations.mutate({
                                challenge_id: id,
                                content: markdown,
                            });
                        }}
                    />
                </div>
            </div>
        </div>
    );
}
