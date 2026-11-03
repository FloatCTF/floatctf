/**
 * SQL 控制台。
 *
 * 这是**最危险的一页**：一条 `DROP TABLE` 就是不可逆的。因此：
 * - 默认只允许 SELECT：非查询语句必须先显式打开"允许写操作"开关；
 * - 执行前确认弹窗会**回显将要执行的 SQL 首行**，避免"我以为是 SELECT"；
 * - 结果以表格呈现（`rows` + `count` + 耗时），错误直接展示后端原文；
 * - 历史保留在页面内，方便对比前后两次的结果。
 *
 * 后端接口是 `exec_sql`（单条语句），因此这里**不做**分号切分 ——
 * 前端切分会在字符串字面量、函数体、注释处出错，那是最糟的一类 bug。
 */
import { useState } from "react";
import { Button, Card, Divider, Input, Notification, Switch, Tag } from "animal-island-ui";
import { AlertTriangleIcon, PlayIcon, TrashIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState } from "~/components/states";
import type { SqlResult } from "@floatctf/sdk";

type HistoryEntry = {
	sql: string;
	at: string;
	ok: boolean;
	result?: SqlResult;
	error?: string;
};

export function ConsoleDatabase() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [sql, setSql] = useState("");
	const [allowWrite, setAllowWrite] = useState(false);
	const [history, setHistory] = useState<HistoryEntry[]>([]);

	const exec = useUniMutation(
		(vars: { sql: string }) => runtime.client.admin.database.exec_sql(vars),
		"执行 SQL",
	);

	/** 粗判是否为只读语句 —— 仅用于决定"是否需要先打开开关"，不是安全边界。 */
	const looksReadOnly = /^\s*(select|with|show|explain|describe|table)\b/i.test(sql);

	async function run() {
		const statement = sql.trim();
		if (!statement) return;

		if (!looksReadOnly && !allowWrite) {
			Notification.error({
				message: "这是写操作",
				description: "请先打开「允许写操作」开关。",
			});
			return;
		}

		const ok = await confirm({
			title: looksReadOnly ? "执行查询" : "执行写操作",
			consequence: (
				<span>
					将对平台数据库执行：
					<div className="ai-terminal" style={{ maxHeight: 120, marginTop: 8 }}>
						{statement.length > 400 ? `${statement.slice(0, 400)}…` : statement}
					</div>
					{looksReadOnly
						? "查询本身不会改数据，但**大表全扫会占用数据库资源**，进而影响线上比赛。"
						: "**这会修改或删除数据，且不可撤销。**没有任何自动备份会替你兜底。"}
				</span>
			),
			okText: looksReadOnly ? "执行" : "执行写操作",
			danger: !looksReadOnly,
		});
		if (!ok) return;

		const entry: HistoryEntry = { sql: statement, at: new Date().toISOString(), ok: false };
		try {
			const result = await exec.mutateAsync({ sql: statement });
			entry.ok = true;
			entry.result = result ?? undefined;
		} catch (e) {
			entry.ok = false;
			entry.error = e instanceof Error ? e.message : String(e);
		}
		setHistory((prev) => [entry, ...prev].slice(0, 20));
	}

	return (
		<>
			<PageHeader
				title="SQL 控制台"
				description="直接对平台数据库执行单条语句。非查询语句需要先显式解锁。"
			/>

			<Card>
				<div className="ai-row" style={{ gap: 8 }}>
					<AlertTriangleIcon size={16} />
					<span className="ai-muted">
						这里执行的是**平台生产数据库**的真实语句。前端的开关只是防误触，
					真正的权限判定在后端（需要 `database.manage`）。
					</span>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<Input
						value={sql}
						placeholder="SELECT 1;"
						onChange={(e) => setSql(e.target.value)}
					/>
					<div className="ai-row-between">
						<div className="ai-row" style={{ gap: 10 }}>
							<Switch
								checked={allowWrite}
								onChange={(checked) => setAllowWrite(checked)}
							/>
							<span>允许写操作（UPDATE / DELETE / DDL …）</span>
							{!looksReadOnly && sql.trim() ? (
								<Tag size="small" variant="soft" color="app-red">
									当前语句看起来不是查询
								</Tag>
							) : null}
						</div>
						<div className="ai-row" style={{ gap: 8 }}>
							<Button
								size="small"
								icon={<TrashIcon size={14} />}
								disabled={history.length === 0}
								onClick={() => setHistory([])}
							>
								清空历史
							</Button>
							<Button
								type="primary"
								icon={<PlayIcon size={16} />}
								loading={exec.isPending}
								disabled={!sql.trim()}
								onClick={() => void run()}
							>
								执行
							</Button>
						</div>
					</div>
				</div>
			</Card>

			{history.length === 0 ? (
				<Card>
					<EmptyState
						title="还没有执行过语句"
						description="执行结果会显示在这里，保留最近 20 条。"
					/>
				</Card>
			) : (
				history.map((h, i) => (
					<Card key={`${h.at}-${i}`}>
						<div className="ai-col" style={{ gap: 10 }}>
							<div className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<Tag
										size="small"
										variant="soft"
										color={h.ok ? "app-green" : "app-red"}
									>
										{h.ok ? "成功" : "失败"}
									</Tag>
									{h.result ? (
										<>
											<Tag size="small" variant="outlined">
												{h.result.sql_type}
											</Tag>
											<span className="ai-muted" style={{ fontSize: 12 }}>
												{h.result.count} 行 ／ 影响 {h.result.rows_affected} ／{" "}
												{h.result.elapsed_ms} ms
											</span>
										</>
									) : null}
								</div>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									{new Date(h.at).toLocaleTimeString()}
								</span>
							</div>
							<div className="ai-terminal" style={{ maxHeight: 90 }}>
								{h.sql}
							</div>
							<Divider />
							{h.error ? (
								<div className="ai-terminal" style={{ maxHeight: 160 }}>
									{h.error}
								</div>
							) : h.result && h.result.rows.length > 0 ? (
								<div className="ai-scroll">
									<table style={{ width: "100%", borderCollapse: "collapse" }}>
										<thead>
											<tr>
												{Object.keys(h.result.rows[0]).map((col) => (
													<th
														key={col}
														style={{
															textAlign: "left",
															padding: "6px 10px",
															borderBottom:
																"2px solid var(--animal-border-color)",
														}}
													>
														{col}
													</th>
												))}
											</tr>
										</thead>
										<tbody>
											{h.result.rows.map((row, ri) => (
												<tr key={ri}>
													{Object.keys(h.result?.rows[0] ?? {}).map((col) => (
														<td
															key={col}
															style={{ padding: "6px 10px" }}
															className="ai-mono"
														>
															{row[col] === null ? (
																<span className="ai-muted">NULL</span>
															) : (
																String(row[col])
															)}
														</td>
													))}
												</tr>
											))}
										</tbody>
									</table>
								</div>
							) : (
								<span className="ai-muted">语句执行成功，没有返回行。</span>
							)}
						</div>
					</Card>
				))
			)}
		</>
	);
}
