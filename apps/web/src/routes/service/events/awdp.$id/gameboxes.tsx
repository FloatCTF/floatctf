import { Button, Label, Spinner } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useRef, useState } from "react";

import { awdpPlayerApi, type AwdpGameBox } from "@/api/awdp";
import { useMsgBanner } from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/gameboxes")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner({});
	const queryClient = useQueryClient();
	const { data, isLoading, isError } = useQuery({
		queryKey: ["awdp-overview", id],
		queryFn: () => awdpPlayerApi.overview(id),
	});
	const phase = data?.data?.phase;
	const gameboxes = data?.data?.gameboxes ?? [];

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["awdp-overview", id] });
	};

	const startMutation = useMutation({
		mutationFn: (egId: string) => awdpPlayerApi.startInstance(id, egId),
		onSuccess: () => {
			banner.showBanner("success", "Instance started");
			invalidate();
		},
		onError: banner.showErrorBanner,
	});
	const stopMutation = useMutation({
		mutationFn: (egId: string) => awdpPlayerApi.stopInstance(id, egId),
		onSuccess: () => {
			banner.showBanner("success", "Instance stopped");
			invalidate();
		},
		onError: banner.showErrorBanner,
	});
	const resetMutation = useMutation({
		mutationFn: (egId: string) => awdpPlayerApi.resetInstance(id, egId),
		onSuccess: () => {
			banner.showBanner("success", "Instance reset (pristine)");
			invalidate();
		},
		onError: banner.showErrorBanner,
	});

	if (isLoading) {
		return <Spinner size="large" />;
	}
	if (isError) {
		return (
			<div className="p-4 flex flex-col gap-2">
				<banner.BannerComponent />
				<InlineMessage variant="warning">
					未获取到 GameBox 列表：请先到 Overview 页加入赛事。
				</InlineMessage>
			</div>
		);
	}

	return (
		<div className="m-2 flex flex-col gap-3">
			<banner.BannerComponent />
			{phase === "break" && (
				<p className="text-sm opacity-80">
					Break 阶段：只能启动实例并提交 flag；源码与 Patch 在 Fix 阶段开放。
				</p>
			)}
			{phase === "fix" && (
				<p className="text-sm opacity-80">
					Fix 阶段：可下载源码、上传 patch.sh（≤256KiB）、运行 Test Check（不计分）。
				</p>
			)}
			{gameboxes.map((gb) => (
				<GameBoxCard
					key={gb.id}
					gb={gb}
					phase={phase ?? "pending"}
					startMutation={startMutation}
					stopMutation={stopMutation}
					resetMutation={resetMutation}
				/>
			))}
			{gameboxes.length === 0 && (
				<p className="text-sm opacity-70">暂无 GameBox。</p>
			)}
		</div>
	);
}

type MutationLike = {
	mutate: (egId: string) => void;
	isPending: boolean;
};

function GameBoxCard({
	gb,
	phase,
	startMutation,
	stopMutation,
	resetMutation,
}: {
	gb: AwdpGameBox;
	phase: string;
	startMutation: MutationLike;
	stopMutation: MutationLike;
	resetMutation: MutationLike;
}) {
	const { id } = Route.useParams();
	const banner = useMsgBanner({});
	const queryClient = useQueryClient();
	const inst = gb.instance;
	const active = phase === "break" || phase === "fix";
	const running = inst?.runtime_state === "running";

	const [patchFile, setPatchFile] = useState<File | null>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const patchMutation = useMutation({
		mutationFn: () => awdpPlayerApi.uploadPatch(id, gb.id, patchFile!),
		onSuccess: (res) => {
			const status = res.data?.status ?? "failed";
			banner.showBanner(
				status === "applied" ? "success" : "critical",
				status === "applied" ? "Patch applied" : "Patch failed",
			);
			setPatchFile(null);
			if (fileInputRef.current) fileInputRef.current.value = "";
			queryClient.invalidateQueries({ queryKey: ["awdp-overview", id] });
		},
		onError: banner.showErrorBanner,
	});

	const [checking, setChecking] = useState(false);
	const [checkResult, setCheckResult] = useState<{
		healthcheck_ok: boolean;
		judge_ok: boolean;
	} | null>(null);
	const testCheckMutation = useMutation({
		mutationFn: () => awdpPlayerApi.testCheck(id, gb.id),
		onMutate: () => {
			setChecking(true);
			setCheckResult(null);
		},
		onSuccess: (res) => {
			setChecking(false);
			const d = res.data;
			if (!d) return;
			setCheckResult({ healthcheck_ok: d.healthcheck_ok, judge_ok: d.judge_ok });
		},
		onError: (e) => {
			setChecking(false);
			banner.showErrorBanner(e);
		},
	});

	const sourceQuery = useQuery({
		queryKey: ["awdp-source-url", id, gb.id],
		queryFn: () => awdpPlayerApi.sourceUrl(id, gb.id),
		enabled: phase === "fix",
	});
	const sourceUrl = sourceQuery.data?.data;

	return (
		<section className="p-3 rounded border">
			<div className="flex items-center gap-2 mb-2">
				<h4 className="font-bold flex-1">{gb.name}</h4>
				<Label variant={gb.broken ? "danger" : "success"}>
					{gb.broken ? "Broken" : "Unbroken"}
				</Label>
				{!gb.enabled && <Label variant="secondary">Disabled</Label>}
				{inst && (
					<Label variant={running ? "success" : "secondary"}>
						{inst.runtime_state}
						{inst.runtime_generation > 1 ? ` (gen ${inst.runtime_generation})` : ""}
					</Label>
				)}
			</div>

			<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1 text-sm mb-2">
				<dt className="font-bold">Category</dt>
				<dd className="font-medium">{gb.category}</dd>
				<dt className="font-bold">Endpoints</dt>
				<dd className="font-medium font-mono text-xs break-all">
					{inst?.endpoints && inst.endpoints.length > 0
						? inst.endpoints
								.map((e) => `${e.protocol}://${e.public_host}:${e.public_port}`)
								.join("  ")
						: (gb.exposed ?? [])
								.map(([proto, port]) => `${proto}:${port} (未启动)`)
								.join("  ") || "-"}
				</dd>
				{gb.source_code_dir && (
					<>
						<dt className="font-bold">Source Dir</dt>
						<dd className="font-medium font-mono text-xs">
							{gb.source_code_dir}
						</dd>
					</>
				)}
			</dl>

			{/* 实例操作 */}
			{active && (
				<div className="flex items-center gap-2 mb-2">
					<Button
						variant="primary"
						disabled={running || startMutation.isPending}
						onClick={() => startMutation.mutate(gb.id)}
					>
						Start
					</Button>
					<Button
						disabled={!running || stopMutation.isPending}
						onClick={() => stopMutation.mutate(gb.id)}
					>
						Stop
					</Button>
					<Button
						variant="danger"
						disabled={!inst || resetMutation.isPending}
						onClick={() => resetMutation.mutate(gb.id)}
					>
						Reset
					</Button>
				</div>
			)}

			{/* Fix 专属：源码下载 / Patch 上传 / Test Check */}
			{phase === "fix" && (
				<div className="flex flex-col gap-2 border-t pt-2">
					<div className="flex items-center gap-2 flex-wrap">
						{sourceUrl ? (
							<a
								href={sourceUrl}
								target="_blank"
								rel="noreferrer"
								className="text-blue-600 hover:underline"
							>
								Download Source
							</a>
						) : (
							<span className="text-sm opacity-60">Fix 阶段可下载源码</span>
						)}
						<input
							ref={fileInputRef}
							type="file"
							accept=".sh,text/x-shellscript"
							className="text-sm"
							onChange={(e) => setPatchFile(e.target.files?.[0] ?? null)}
						/>
						<Button
							variant="primary"
							disabled={!patchFile || patchMutation.isPending}
							onClick={() => patchMutation.mutate()}
						>
							{patchMutation.isPending ? "Applying…" : "Apply Patch"}
						</Button>
						<Button
							disabled={checking || testCheckMutation.isPending || !running}
							onClick={() => testCheckMutation.mutate()}
						>
							{checking ? "Checking…" : "Test Check"}
						</Button>
					</div>
					{checkResult && (
						<div className="text-sm">
							<span className={checkResult.healthcheck_ok ? "text-green-600" : "text-red-600"}>
								健康检查：{checkResult.healthcheck_ok ? "OK" : "DOWN"}
							</span>
							<span className="mx-2">·</span>
							<span className={checkResult.judge_ok ? "text-green-600" : "text-red-600"}>
								Judge：{checkResult.judge_ok ? "PASS" : "FAIL"}
							</span>
							<span className="ml-2 text-xs opacity-60">（不计分）</span>
						</div>
					)}
					<banner.BannerComponent />
				</div>
			)}
		</section>
	);
}
