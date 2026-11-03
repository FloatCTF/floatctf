/**
 * 我的资料。
 *
 * 两个刻意的设计：
 * 1. **密码永不回显** —— 输入框永远为空、只写不读。响应 DTO 里根本没有
 *    password 字段（v2 起如此），因此这里也不可能"顺手显示出来"。
 * 2. 展示**我的有效权限**。选手看到"我为什么看不到某个入口"是有价值的；
 *    同时文案写明这只是展示，真正的判定在服务端 —— 免得有人以为前端在授权。
 */
import { useState } from "react";
import { Avatar, Button, Card, Divider, Input, Notification, Tag } from "animal-island-ui";
import { KeyIcon, LockIcon, RefreshIcon, SaveIcon, ShieldIcon, UserIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { ErrorState, LoadingState } from "~/components/states";
import { useSession } from "~/auth/store";
import { formatTime } from "~/lib/event";

export function ProfilePage() {
	const runtime = useRuntime();
	const setMe = useSession((s) => s.setMe);
	const me = useSession((s) => s.me);

	const [nickname, setNickname] = useState("");
	const [email, setEmail] = useState("");
	const [avatar, setAvatar] = useState("");
	const [password, setPassword] = useState("");
	const [touched, setTouched] = useState(false);

	const query = useUniQuery(
		["island", "me"],
		() => runtime.client.service.users.getMe(),
		"加载资料",
	);

	const patch = useUniMutation(
		(vars: { nickname?: string; email?: string; avatar?: string; password?: string }) =>
			runtime.client.service.users.patchMe(vars),
		"保存资料",
	);

	if (query.isLoading) return <LoadingState label="加载资料…" />;
	if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;

	const profile = query.data;
	if (!profile) {
		return <ErrorState error={new Error("后端没有返回账号资料")} onRetry={() => void query.refetch()} />;
	}

	const effNickname = touched ? nickname : profile.nickname;
	const effEmail = touched ? email : profile.email;
	const effAvatar = touched ? avatar : (profile.avatar ?? "");

	async function save() {
		const payload: Record<string, string> = {};
		if (effNickname !== profile?.nickname) payload.nickname = effNickname;
		if (effEmail !== profile?.email) payload.email = effEmail;
		if (effAvatar !== (profile?.avatar ?? "")) payload.avatar = effAvatar;
		if (password) payload.password = password;

		if (Object.keys(payload).length === 0) {
			Notification.info({ message: "没有要保存的改动" });
			return;
		}

		// 改密会吊销其它会话（后端会自增 token_version），因此必须说清楚。
		if (payload.password) {
			Notification.info({
				message: "正在修改密码",
				description: "修改密码会使其它已登录的会话失效。",
			});
		}

		const updated = await patch.mutateAsync(payload);
		setMe(updated as Parameters<typeof setMe>[0]);
		setPassword("");
		setTouched(false);
		Notification.success({ message: "已保存" });
		void query.refetch();
	}

	return (
		<>
			<PageHeader
				title="我的资料"
				description="账号信息、密码与当前生效的权限。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => void query.refetch()}
					>
						刷新
					</Button>
				}
			/>

			<Card>
				<div className="ai-row" style={{ gap: 16 }}>
					<Avatar size={64} src={effAvatar || undefined}>
						{effNickname?.[0] ?? profile.username[0]}
					</Avatar>
					<div className="ai-col" style={{ gap: 2 }}>
						<span className="ai-strong" style={{ fontSize: 18 }}>
							{profile.nickname}
						</span>
						<span className="ai-muted">@{profile.username}</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							注册于 {formatTime(profile.created_at)}
						</span>
					</div>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong">基本资料</span>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>昵称</span>
						<Input
							prefix={<UserIcon size={16} />}
							value={effNickname}
							onChange={(e) => {
								setTouched(true);
								setNickname(e.target.value);
							}}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>邮箱</span>
						<Input
							value={effEmail}
							onChange={(e) => {
								setTouched(true);
								setEmail(e.target.value);
							}}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>头像 URL</span>
						<Input
							allowClear
							placeholder="https://…"
							value={effAvatar}
							onChange={(e) => {
								setTouched(true);
								setAvatar(e.target.value);
							}}
						/>
					</label>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong">修改密码</span>
					<span className="ai-muted">
						留空表示不改。修改密码会**使其它已登录的会话失效**（后端会递增令牌版本）。
					</span>
					<Input
						type="password"
						allowClear
						placeholder="新密码"
						prefix={<LockIcon size={16} />}
						value={password}
						onChange={(e) => setPassword(e.target.value)}
					/>
				</div>
			</Card>

			<Card>
				<div className="ai-row" style={{ justifyContent: "flex-end" }}>
					<Button
						type="primary"
						icon={<SaveIcon size={16} />}
						loading={patch.isPending}
						onClick={() => void save()}
					>
						保存
					</Button>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<div className="ai-row" style={{ gap: 8 }}>
						<ShieldIcon size={18} />
						<span className="ai-strong">我当前生效的权限</span>
						<Tag size="small" variant="soft" color="app-teal">
							{(me?.permissions?.length ?? 0) + (me?.grants_all ? 1 : 0)} 项
						</Tag>
					</div>
					<Divider />
					{/* 这段话是给用户看的，防止把展示当成授权依据。 */}
					<span className="ai-muted">
						这份列表只用来决定"哪些入口显示给你"。**真正的权限判定在服务端**：
						即使某个入口被隐藏，直接访问 URL 也会被服务端按同一套规则拒绝。
					</span>
					<div className="ai-row" style={{ gap: 6 }}>
						{me?.grants_all ? (
							<Tag size="small" variant="solid" color="app-red">
								全权（含将来新增的权限）
							</Tag>
						) : null}
						{(me?.permissions ?? []).map((p) => (
							<Tag key={p} size="small" variant="outlined">
								{p}
							</Tag>
						))}
						{!me?.grants_all && (me?.permissions?.length ?? 0) === 0 ? (
							<span className="ai-muted">你目前只有基础的登录权限。</span>
						) : null}
					</div>
					<div className="ai-row" style={{ gap: 6 }}>
						<KeyIcon size={16} />
						<span className="ai-muted" style={{ fontSize: 12 }}>
							需要额外权限请联系管理员在控制台的「角色与权限」里授予。
						</span>
					</div>
				</div>
			</Card>
		</>
	);
}
