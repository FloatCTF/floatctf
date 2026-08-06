import { Button, Label, ProgressBar, Spinner, TextInput } from "@primer/react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import dayjs from "dayjs";
import utc from "dayjs/plugin/utc";
import { useEffect, useRef, useState } from "react";
dayjs.extend(utc);

import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import type { Instances } from "@/entity";

export const Route = createFileRoute("/service/challenges/$id/")({
    component: RouteComponent,
});

function RouteComponent() {
    const { id } = Route.useParams();
    const {
        data: challenge_data,
        isLoading,
        isError,
    } = useQuery({
        queryKey: ["challenge", id],
        queryFn: () => serviceApi.challenges.get(id),
    });
    const banner = useMsgBanner();

    const challenge = challenge_data?.data;
    const challengeStatus = useReactive({
        flag: "",
        isRunning: false,
        instance: {} as Instances,
    });

    const { data: instance_data } = useQuery({
        queryKey: ["instance", id],
        queryFn: () => serviceApi.challenges.getInstance(id),
    });

    const mutationInstance = useMutation({
        mutationFn: serviceApi.instances.launch,
        onSuccess: (data) => {
            challengeStatus.isRunning = true;
            challengeStatus.instance = data.data!;
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });
    const destroyInstance = useMutation({
        mutationFn: serviceApi.instances.destroy,
        onSuccess: (_data) => {
            challengeStatus.isRunning = false;
            challengeStatus.instance = {} as Instances;
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });
    const submitFlag = useMutation({
        mutationFn: serviceApi.submit.submit,
        onSuccess: (_data) => {
            banner.showBanner("success", "Flag is correct!");
            // close in the backend
            challengeStatus.isRunning = false;
            challengeStatus.instance = {} as Instances;
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });
    const navigate = useNavigate();
    // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
    useEffect(() => {
        if (instance_data?.data) {
            challengeStatus.isRunning = true;
            challengeStatus.instance = instance_data.data;
        } else {
            challengeStatus.isRunning = false;
            challengeStatus.instance = {} as Instances;
        }
    }, [instance_data]);
    // 顶层调用 Hook
    useTitle(`${challenge?.name ?? "Challenge"} | FloatCTF`);

    if (isError) {
        navigate({ to: "/service/challenges" });
    }

    if (isLoading) {
        return <Spinner size="large" />;
    }

    return (
        <div className="h-full w-full flex flex-col gap-2 justify-between">
            <div id="challenge-meta" className="flex-6">
                <p className="font-bold text-2xl">{challenge?.name}</p>
                <div className="border-top mt-2 pt-2">
                    {challenge?.description}
                </div>
            </div>
            <banner.BannerComponent />
            {/* 附件 */}
            {challenge?.attachment && (
                <a
                    href={`/static/challenges/${challenge.safe_name}/${challenge.attachment}`}
                    download
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-block mt-1"
                >
                    <Label variant="accent">{challenge.attachment}</Label>
                </a>
            )}
            {challengeStatus.instance && (
                <div
                    // biome-ignore lint/security/noDangerouslySetInnerHtml: <explanation>
                    dangerouslySetInnerHTML={{
                        __html: challengeStatus.instance.content || "",
                    }}
                />
            )}
            <div
                id="challenge-content"
                className="mb-4 flex justify-center flex-1 border-bottom"
            >
                {challengeStatus.isRunning ? (
                    <div className="w-full flex flex-col gap-2 mb-4">
                        <RemainingTimer
                            destroy_at={challengeStatus.instance.destroy_at}
                            onExpire={() => {
                                destroyInstance.mutate(
                                    challengeStatus.instance.id,
                                );
                            }}
                        />

                        <div className="flex gap-2">
                            <TextInput
                                className="flex-1"
                                value={challengeStatus.flag}
                                onChange={(e) => {
                                    challengeStatus.flag = e.target.value;
                                }}
                                placeholder="flag{}"
                            />
                            <Button
                                variant="danger"
                                onClick={() => {
                                    destroyInstance.mutate(
                                        challengeStatus.instance.id,
                                    );
                                }}
                            >
                                Destroy
                            </Button>

                            <Button
                                variant="primary"
                                onClick={() => {
                                    submitFlag.mutate({
                                        instance_id:
                                            challengeStatus.instance.id,
                                        flag: challengeStatus.flag,
                                    });
                                    // need clear the flag value
                                    challengeStatus.flag = "";
                                }}
                            >
                                Submit
                            </Button>
                        </div>
                    </div>
                ) : (
                    <Button
                        variant="primary"
                        onClick={() => {
                            mutationInstance.mutate(id);
                        }}
                    >
                        Launch
                    </Button>
                )}
            </div>
        </div>
    );
}

type RemainingTimerProps = {
    destroy_at: string;
    totalMinutes?: number;
    onExpire?: () => void;
};

export const RemainingTimer = ({
    destroy_at,
    totalMinutes = 60,
    onExpire,
}: RemainingTimerProps) => {
    const [remaining, setRemaining] = useState({
        formatted: "",
        percentage: 100,
    });

    // ✅ 防止 onExpire 重复调用
    const expiredRef = useRef(false);

    useEffect(() => {
        const update = () => {
            const destroyTime = dayjs.utc(destroy_at).toDate();
            const diffSeconds = Math.floor(
                (destroyTime.getTime() - Date.now()) / 1000,
            );

            const safeDiff = Math.max(diffSeconds, 0);
            const minutes = Math.floor(safeDiff / 60);
            const seconds = safeDiff % 60;
            // ✅ <= 0 确保不会错过触发点
            setRemaining({
                formatted: `${minutes}m ${seconds}s`,
                percentage: Math.max(
                    Math.min((safeDiff / (totalMinutes * 60)) * 100, 100),
                    0,
                ),
            });

            if (safeDiff <= 0 && !expiredRef.current) {
                expiredRef.current = true;
                console.log("🔥 Expired, calling onExpire()");
                onExpire?.();
            }
        };

        update(); // 初始化跑一次
        const timer = setInterval(update, 1000);
        return () => clearInterval(timer);
    }, [destroy_at, totalMinutes, onExpire]);

    return (
        <>
            <span
                style={{
                    color: "var(--fgColor-muted)",
                    font: "var(--text-body-shorthand-medium)",
                }}
                className="text-center"
            >
                Remaining: {remaining.formatted}
            </span>
            <ProgressBar
                animated
                progress={remaining.percentage}
                aria-label="remaining"
            />
        </>
    );
};
