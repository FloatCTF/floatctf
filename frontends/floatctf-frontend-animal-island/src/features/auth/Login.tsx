/**
 * 登录页 —— 唯一入口（`POST /api/session`）。
 *
 * v2 身份模型：**一个账号体系**，没有"管理端登录"与"选手登录"之分。
 * 登录只回答「你是谁」；能不能进控制台由 `/me` 的权限决定（见顶栏的工作区切换）。
 *
 * 因此这里**不**在登录时校验管理权限 —— 那会让"选手"无法登录。
 * 前端只负责把凭据换成会话，然后如实展示账号实际拥有的能力。
 */
import { useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { Button, Card, Divider, Input, Modal, Notification, Title, Tooltip } from "animal-island-ui";
import { KeyIcon, LockIcon, MailIcon, UserIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { errorText } from "~/api/errors";
import { useSession } from "~/auth/store";

export function LoginPage() {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const [params] = useSearchParams();
	const setToken = useSession((s) => s.setToken);
	const setMe = useSession((s) => s.setMe);

	const [username, setUsername] = useState("");
	const [password, setPassword] = useState("");
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);

	// 忘记密码：用户名或邮箱**任填其一**即可申请重置。
	const [resetOpen, setResetOpen] = useState(false);
	const [resetUsername, setResetUsername] = useState("");
	const [resetEmail, setResetEmail] = useState("");
	const [resetBusy, setResetBusy] = useState(false);

	async function submit() {
		if (busy) return;
		setError(null);
		setBusy(true);
		try {
			const res = await runtime.client.service.users.login({ username, password });
			const token = (res as { data?: string | null }).data;
			if (!token) {
				// 登录成功却拿不到令牌是契约问题，必须显式失败而不是"看起来登录了"。
				throw new Error("登录响应没有返回会话令牌");
			}
			setToken(token);
			// 立刻拉一次 /me：导航显隐与顶栏身份都依赖它。
			// 失败不阻断登录 —— 页面自己会再拉到，或由 401 引导重新登录。
			try {
				const me = await runtime.client.service.users.getMe();
				setMe((me as { data?: unknown }).data as Parameters<typeof setMe>[0]);
			} catch {
				/* 忽略：/me 失败由 SessionBootstrap 与页面查询兜底 */
			}
			const next = params.get("next");
			// 只接受站内相对路径，避免开放重定向。
			const target = next && next.startsWith("/") && !next.startsWith("//") ? next : "/island";
			void navigate(target, { replace: true });
		} catch (e) {
			setError(errorText(e, "登录失败"));
		} finally {
			setBusy(false);
		}
	}

	return (
		<div
			style={{
				minHeight: "100%",
				display: "flex",
				alignItems: "center",
				justifyContent: "center",
				padding: 24,
				background: "var(--animal-bg-color)",
			}}
		>
			<div style={{ width: "100%", maxWidth: 420 }} className="ai-col">
				<div className="ai-col" style={{ alignItems: "center", gap: 4 }}>
					<Title size="large" color="app-teal">
						FloatCTF
					</Title>
					<span className="ai-muted">动物岛 · 登录你的账号</span>
				</div>

				<Card>
					<div className="ai-col" style={{ gap: 14 }}>
						<label className="ai-col" style={{ gap: 6 }}>
							<span className="ai-strong">用户名</span>
							<Input
								size="large"
								autoFocus
								allowClear
								prefix={<UserIcon size={16} />}
								placeholder="你的用户名"
								value={username}
								onChange={(e) => setUsername(e.target.value)}
								onKeyDown={(e) => {
									if (e.key === "Enter") void submit();
								}}
							/>
						</label>

						<label className="ai-col" style={{ gap: 6 }}>
							<span className="ai-strong">密码</span>
							<Input
								size="large"
								type="password"
								prefix={<LockIcon size={16} />}
								placeholder="你的密码"
								value={password}
								onChange={(e) => setPassword(e.target.value)}
								onKeyDown={(e) => {
									if (e.key === "Enter") void submit();
								}}
							/>
						</label>

						{error ? (
							<div
								role="alert"
								style={{
									background: "#fdeceb",
									border: "2px solid var(--animal-error-color)",
									borderRadius: "var(--animal-border-radius-sm)",
									padding: "8px 12px",
									color: "var(--animal-error-color)",
								}}
							>
								{error}
							</div>
						) : null}

						<Button
							type="primary"
							size="large"
							block
							loading={busy}
							icon={<KeyIcon size={18} />}
							onClick={() => void submit()}
						>
							{busy ? "登录中…" : "登录"}
						</Button>

						<div className="ai-row" style={{ justifyContent: "space-between" }}>
							<Button
								type="text"
								size="small"
								onClick={() => {
									// 把已输入的用户名带过去，少让用户打一遍。
									setResetUsername(username);
									setResetEmail("");
									setResetOpen(true);
								}}
							>
								忘记密码？
							</Button>
							<Tooltip title="账号由管理员创建；平台不开放自助注册">
								<span className="ai-muted" style={{ fontSize: 12 }}>
									没有账号？
								</span>
							</Tooltip>
						</div>

						<Divider />

						<div className="ai-row-between">
							<span className="ai-muted" style={{ fontSize: 12 }}>
								还没有账号？请联系管理员创建。
							</span>
							<Tooltip title={`前端 ${runtime.frontendVersion} · 平台 ${runtime.platformVersion}`}>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									动物岛前端
								</span>
							</Tooltip>
						</div>
					</div>
				</Card>
			</div>

			{/* 重置申请走真正的 Modal（不跳路由）：它是一次性的辅助动作，
			    为它做一个整页会让"我只是打错密码"变重。 */}
			<Modal
				open={resetOpen}
				title="申请重置密码"
				typewriter={false}
				onClose={() => setResetOpen(false)}
				footer={
					<>
						<Button onClick={() => setResetOpen(false)}>取消</Button>
						<Button
							type="primary"
							disabled={!resetUsername.trim() && !resetEmail.trim()}
							loading={resetBusy}
							onClick={async () => {
								setResetBusy(true);
								try {
									await runtime.client.service.users.resetPassword({
										username: resetUsername.trim() || undefined,
										email: resetEmail.trim() || undefined,
									});
									// 无论账号是否存在都返回同样的结果 —— 这是刻意的：
									// 区分"已发送"与"账号不存在"会变成账号枚举接口。
									Notification.success({
										message: "重置申请已提交",
										description: "如果该账号存在，重置说明会发送到它的邮箱。",
									});
									setResetOpen(false);
								} catch (e) {
									Notification.error({
										message: "提交失败",
										description: errorText(e, "请稍后重试。"),
									});
								} finally {
									setResetBusy(false);
								}
							}}
						>
							提交申请
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-muted">
						填写**用户名或邮箱**其中一项即可。出于安全考虑，平台不会告诉你该账号是否存在。
					</span>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>用户名</span>
						<Input
							allowClear
							prefix={<UserIcon size={16} />}
							value={resetUsername}
							onChange={(e) => setResetUsername(e.target.value)}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>邮箱</span>
						<Input
							allowClear
							prefix={<MailIcon size={16} />}
							value={resetEmail}
							onChange={(e) => setResetEmail(e.target.value)}
						/>
					</label>
				</div>
			</Modal>
		</div>
	);
}
