import { PencilIcon } from "@primer/octicons-react";
import { Avatar, Button, FormControl, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";
import { useEffect, useRef, useState } from "react";

import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import type { Users } from "@/entity";
import { diffToPatch } from "@/util";

export const Route = createFileRoute("/service/profile")({
	component: RouteComponent,
});

function RouteComponent() {
	const queryClient = useQueryClient();
	const [editable, setEditable] = useState(false);
	const { data, isLoading } = useQuery({
		queryKey: ["profile"],
		queryFn: () => serviceApi.users.getMe(),
		// 低频数据：个人资料 5 分钟缓存（覆盖全局 30s）；
		// 更新资料后下方 mutation 会 invalidate 强制刷新。
		staleTime: 5 * 60_000,
		select: (res) => res.data,
	});

	const [originalProfile, setOriginalProfile] = useState<Partial<Users>>({});
	const banner = useMsgBanner();

	// Avatar upload state
	const [pendingFile, setPendingFile] = useState<File | null>(null);
	const [previewUrl, setPreviewUrl] = useState<string>("");
	const fileInputRef = useRef<HTMLInputElement>(null);

	const mutationProfile = useReactive<Partial<Users>>({
		username: "",
		nickname: "",
		email: "",
		password: "",
	});

	const patchMutation = useMutation({
		mutationFn: serviceApi.users.patchMe,
		onSuccess: () => {
			setEditable(false);
			banner.showBanner("success", "Update profile successfully!");
		},
		onError: (e: AxiosError<{ msg: string }>) => {
			if (e.response?.data) {
				banner.showBanner("critical", e.response?.data.msg);
			}
		},
	});

	const avatarUploadMutation = useMutation({
		mutationFn: serviceApi.uploads.upload_avatar,
		onSuccess: () => {
			setPendingFile(null);
			setPreviewUrl("");
			queryClient.invalidateQueries({ queryKey: ["profile"] });
			banner.showBanner("success", "Avatar updated successfully!");
		},
		onError: () => {
			banner.showBanner("critical", "Failed to upload avatar");
		},
	});

	const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
		const file = e.target.files?.[0];
		if (!file) return;

		setPendingFile(file);
		setPreviewUrl(URL.createObjectURL(file));

		// Clear input so selecting the same file again triggers onChange
		e.target.value = "";
	};

	const handleUploadAvatar = () => {
		if (pendingFile) {
			avatarUploadMutation.mutate(pendingFile);
		}
	};

	const handleCancelPreview = () => {
		setPendingFile(null);
		URL.revokeObjectURL(previewUrl);
		setPreviewUrl("");
	};

	// Cleanup object URL on unmount
	useEffect(() => {
		return () => {
			if (previewUrl) URL.revokeObjectURL(previewUrl);
		};
	}, [previewUrl]);

	useEffect(() => {
		if (data) {
			Object.assign(mutationProfile, data);
			setOriginalProfile(data);
		}
	}, [data, mutationProfile]);

	if (isLoading) {
		return <div>Loading...</div>;
	}

	// Determine the avatar source: pending preview > server avatar
	const avatarSrc = previewUrl || data?.avatar || "";

	return (
		<div className="px-3 w-full max-w-[1100px]">
			{/* Page title + divider（GitHub Settings 平面标题） */}
			<h3 className="text-2xl font-semibold py-2 my-3 border-b border-[var(--borderColor-default)]">
				Profile
			</h3>

			{/* 两栏：左侧表单 | 右侧头像（窄屏单列，头像在前） */}
			<div className="grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_280px] gap-8 md:gap-12 mt-4 items-start">
				{/* ── 左侧：表单列 ── */}
				<div className="flex flex-col gap-6 min-w-0 max-w-[600px] md:order-1">
					<FormControl>
						<FormControl.Label>Username</FormControl.Label>
						<TextInput
							value={mutationProfile.username}
							disabled={true}
							onChange={(e) => {
								mutationProfile.username = e.target.value;
							}}
						/>
						<FormControl.Caption>
							This is can not mutate, or contact the administrator
						</FormControl.Caption>
					</FormControl>
					<FormControl>
						<FormControl.Label>Nickname</FormControl.Label>
						<TextInput
							value={mutationProfile.nickname}
							disabled={!editable}
							onChange={(e) => {
								mutationProfile.nickname = e.target.value;
							}}
						/>
						<FormControl.Caption>
							The nickname will be displayed in the public area
						</FormControl.Caption>
					</FormControl>
					<FormControl>
						<FormControl.Label>Email</FormControl.Label>
						<TextInput
							value={mutationProfile.email}
							disabled={!editable}
							onChange={(e) => {
								mutationProfile.email = e.target.value;
							}}
						/>
					</FormControl>
					<FormControl>
						<FormControl.Label>Password</FormControl.Label>
						<TextInput
							value={mutationProfile.password}
							disabled={!editable}
							onChange={(e) => {
								mutationProfile.password = e.target.value;
							}}
						/>
						<FormControl.Caption>Fill it</FormControl.Caption>
					</FormControl>
					{editable && (
						<div className="flex gap-2 mt-2">
							<Button variant="danger" onClick={() => setEditable(false)}>
								Cancel
							</Button>
							<Button
								variant="primary"
								onClick={() => {
									const payload = diffToPatch(originalProfile, mutationProfile);
									patchMutation.mutate(payload);
								}}
							>
								Save
							</Button>
						</div>
					)}
					<banner.BannerComponent />
				</div>

				{/* ── 右侧：头像列 ── */}
				<div className="flex flex-col items-start gap-3 md:order-2 min-w-0">
					<span className="text-base font-semibold">Profile picture</span>
					<div
						className="relative group cursor-pointer inline-block rounded-full"
						onClick={() => fileInputRef.current?.click()}
					>
						{avatarSrc ? (
							<Avatar
								src={avatarSrc}
								size={{ narrow: 160, regular: 200 }}
								alt="Avatar"
							/>
						) : (
							<div className="w-[160px] h-[160px] md:w-[200px] md:h-[200px] rounded-full bg-gray-200 flex items-center justify-center border border-gray-300">
								<span className="text-4xl font-medium text-gray-500">
									{data?.nickname?.[0]?.toUpperCase() || "?"}
								</span>
							</div>
						)}

						{/* Hover overlay */}
						<div className="absolute inset-0 bg-black/50 rounded-full opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity z-10">
							<span className="text-white text-sm font-medium">Select</span>
						</div>
					</div>

					{/* Hidden file input */}
					<input
						ref={fileInputRef}
						type="file"
						accept="image/*"
						className="hidden"
						onChange={handleFileChange}
					/>

					{/* Upload / Cancel buttons (shown only when a new file is selected) */}
					{pendingFile && (
						<div className="flex gap-2">
							<Button
								size="small"
								variant="primary"
								onClick={handleUploadAvatar}
								disabled={avatarUploadMutation.isPending}
							>
								{avatarUploadMutation.isPending ? "Uploading..." : "Upload"}
							</Button>
							<Button
								size="small"
								variant="danger"
								onClick={handleCancelPreview}
							>
								Cancel
							</Button>
						</div>
					)}

					{!editable && (
						<Button className="w-fit" onClick={() => setEditable(true)}>
							<PencilIcon />
							&emsp;Edit
						</Button>
					)}
				</div>
			</div>
		</div>
	);
}
