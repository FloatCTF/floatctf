/**
 * 管理端 · SQL 控制台（`/admin/infra/sql`，specialized 能力，**极高危**）。
 *
 * 真实接口：`client.admin.database.exec_sql({ sql })` → `SqlResult`。
 *
 * 与后端握手的三条硬事实（`apps/api/src/modules/platform/operations/database.rs`）：
 * 1. 单次请求**只接受一条语句**：含内嵌 `;` 会被 400 拒绝（"Multi-statement SQL is not
 *    allowed"）⇒ 本页在前端按语句切分后**逐条**执行，并逐条展示结果 / 错误。
 * 2. 只有 `select/show/describe/explain/with` 视为只读；写语句必须带
 *    `ADMIN_CONFIRMED` 注释前缀（形如 `/* ADMIN_CONFIRMED ...`，见常量
 *    `ADMIN_CONFIRMED_PREFIX`），否则 400。本页在用户通过 `confirmPhrase` 确认后
 *    才为写语句补上该前缀（确认词只在 UI 层，前缀是后端契约）。
 * 3. 功能受 TOML `[features].unsafe_sql_admin = true` 门控；关闭时返回 404 +
 *    中文说明，本页把后端原文原样显示。
 * 另外：单条语句长度上限 10_000 字符、执行超时 5s（都在前端预先提示）。
 */

import { useState, type ReactNode } from "react";
import { AlertTriangle, Eraser, Play, ShieldAlert } from "lucide-react";

import type { SqlResult } from "@floatctf/sdk";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { errorText } from "~/api/errors";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import {
	MonoText,
	PageBody,
	PageHeader,
	ReadonlyBlock,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import { EmptyBlock } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Textarea } from "~/components/ui/textarea";
import { formatInt, truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

/** 与后端一致：单条语句最大长度 / 超时。 */
const MAX_SQL_LENGTH = 10_000;
const SQL_TIMEOUT_SECS = 5;
/** 写语句必须带的后端确认前缀。 */
const ADMIN_CONFIRMED_PREFIX = "/* ADMIN_CONFIRMED */";
/** 执行前必须输入的确认词。 */
const EXECUTE_PHRASE = "EXECUTE";

const READ_ONLY_COMMANDS = new Set(["select", "show", "describe", "explain", "with"]);

const EXAMPLES = [
	"SELECT datname FROM pg_database WHERE datistemplate = false;",
	"SELECT tablename FROM pg_tables WHERE schemaname NOT IN ('pg_catalog','information_schema');",
	"SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_name = 'users';",
	"SELECT current_database(), current_user;",
];

interface StatementOutcome {
	/** 实际提交给后端的 SQL（写语句已带 ADMIN_CONFIRMED 前缀）。 */
	sql: string;
	ok: boolean;
	result?: SqlResult;
	errorMessage?: string;
}

/**
 * 按 `;` 切分语句，但忽略字符串字面量、`$$` / `$tag$` 美元引用与注释里的分号。
 * 这是前端交互所需的最小实现：切分后每条语句单独提交（后端禁止多语句）。
 */
export function splitStatements(input: string): string[] {
	const statements: string[] = [];
	let current = "";
	let index = 0;
	let inSingle = false;
	let inDouble = false;
	let inLineComment = false;
	let inBlockComment = false;
	let dollarTag: string | null = null;

	while (index < input.length) {
		const char = input[index];
		const next = input[index + 1];

		if (inLineComment) {
			current += char;
			if (char === "\n") inLineComment = false;
			index += 1;
			continue;
		}
		if (inBlockComment) {
			current += char;
			if (char === "*" && next === "/") {
				current += next;
				index += 2;
				inBlockComment = false;
				continue;
			}
			index += 1;
			continue;
		}
		if (dollarTag !== null) {
			if (input.startsWith(dollarTag, index)) {
				current += dollarTag;
				index += dollarTag.length;
				dollarTag = null;
				continue;
			}
			current += char;
			index += 1;
			continue;
		}
		if (inSingle) {
			current += char;
			if (char === "'") {
				if (next === "'") {
					current += next;
					index += 2;
					continue;
				}
				inSingle = false;
			}
			index += 1;
			continue;
		}
		if (inDouble) {
			current += char;
			if (char === '"') inDouble = false;
			index += 1;
			continue;
		}
		if (char === "-" && next === "-") {
			current += char + next;
			index += 2;
			inLineComment = true;
			continue;
		}
		if (char === "/" && next === "*") {
			current += char + next;
			index += 2;
			inBlockComment = true;
			continue;
		}
		if (char === "'") {
			inSingle = true;
			current += char;
			index += 1;
			continue;
		}
		if (char === '"') {
			inDouble = true;
			current += char;
			index += 1;
			continue;
		}
		if (char === "$") {
			const match = /^(?:\$\$|\$[A-Za-z_][A-Za-z0-9_]*\$)/.exec(input.slice(index));
			if (match) {
				dollarTag = match[0];
				current += dollarTag;
				index += dollarTag.length;
				continue;
			}
		}
		if (char === ";") {
			statements.push(current);
			current = "";
			index += 1;
			continue;
		}
		current += char;
		index += 1;
	}

	statements.push(current);
	return statements.map((statement) => statement.trim()).filter((statement) => statement.length > 0);
}

/** 与后端相同的命令识别（取首个空白分隔 token 的小写形式）。 */
function firstCommand(sql: string): string {
	return sql.trimStart().split(/\s+/)[0]?.toLowerCase() ?? "";
}

function isReadOnly(sql: string): boolean {
	return READ_ONLY_COMMANDS.has(firstCommand(sql));
}

/** 写语句补上后端要求的确认前缀（幂等）。 */
function withConfirmationPrefix(sql: string): string {
	if (sql.trimStart().toUpperCase().startsWith(ADMIN_CONFIRMED_PREFIX)) return sql;
	return `${ADMIN_CONFIRMED_PREFIX} ${sql}`;
}

function cellText(value: unknown): string {
	if (value === null || value === undefined) return "NULL";
	if (typeof value === "string") return value;
	if (typeof value === "number" || typeof value === "boolean") return String(value);
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

export function AdminSqlPage(): ReactNode {
	useDocumentTitle("SQL 控制台 · FloatCTF 控制台");
	const client = useClient();
	const confirm = useConfirm();
	const [sql, setSql] = useState("");
	const [running, setRunning] = useState(false);
	const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
	const [outcomes, setOutcomes] = useState<StatementOutcome[]>([]);
	const [executedAt, setExecutedAt] = useState<string | null>(null);

	const statements = splitStatements(sql);
	const writeCount = statements.filter((statement) => !isReadOnly(statement)).length;

	async function execute() {
		const parsed = splitStatements(sql);
		if (parsed.length === 0) {
			toast.warning("请输入至少一条 SQL 语句");
			return;
		}
		const tooLong = parsed.filter((statement) => statement.length > MAX_SQL_LENGTH);
		const writes = parsed.filter((statement) => !isReadOnly(statement));

		const ok = await confirm({
			title: `执行 ${parsed.length} 条 SQL？`,
			description:
				writes.length > 0
					? `其中 ${writes.length} 条为写语句（前端会补上后端要求的 ADMIN_CONFIRMED 前缀）`
					: "全部为只读查询（select / show / describe / explain / with）",
			consequences: [
				"语句在平台生产数据库上**真实执行**，无法撤销（写语句会立即提交）",
				"删除 / 更新表数据会导致选手成绩、赛事配置、账号等不可恢复的损坏",
				"单条语句超过 5 秒会被后端中断；执行会在审计日志中留下记录（不记录 SQL 正文）",
				tooLong.length > 0
					? `${tooLong.length} 条语句超过 ${MAX_SQL_LENGTH} 字符上限，会被后端直接拒绝`
					: "确认后按顺序逐条执行，任一条失败不会中断后续语句",
			],
			tone: "danger",
			confirmText: "执行",
			confirmPhrase: EXECUTE_PHRASE,
		});
		if (!ok) return;

		setRunning(true);
		setOutcomes([]);
		setProgress({ done: 0, total: parsed.length });
		const collected: StatementOutcome[] = [];

		for (const [index, statement] of parsed.entries()) {
			const submitted = isReadOnly(statement) ? statement : withConfirmationPrefix(statement);
			try {
				if (statement.length > MAX_SQL_LENGTH) {
					throw new Error(
						`语句长度 ${statement.length} 超过后端上限 ${MAX_SQL_LENGTH} 字符，未提交`,
					);
				}
				const result = await call<SqlResult>(
					client.admin.database.exec_sql({ sql: submitted }),
					`第 ${index + 1} 条语句`,
				);
				collected.push({ sql: submitted, ok: true, result });
			} catch (error) {
				collected.push({
					sql: submitted,
					ok: false,
					errorMessage: errorText(error, "执行失败"),
				});
			}
			setOutcomes([...collected]);
			setProgress({ done: index + 1, total: parsed.length });
		}

		setExecutedAt(new Date().toISOString());
		setProgress(null);
		setRunning(false);
		const failed = collected.filter((item) => !item.ok).length;
		if (failed === 0) {
			toast.success(`全部 ${collected.length} 条语句执行完成`);
		} else {
			toast.error(
				`${collected.length - failed} 条成功，${failed} 条失败`,
				"逐条结果与后端错误见下方列表。",
			);
		}
	}

	return (
		<PageBody>
			<PageHeader
				title="SQL 控制台"
				description="直接对平台 PostgreSQL 执行语句。仅限 superadmin，且需要 TOML 打开 [features].unsafe_sql_admin。"
				badge={<TonePill tone="danger">极高危</TonePill>}
				actions={
					<Toolbar>
						<Button
							variant="outline"
							size="sm"
							disabled={running || sql.length === 0}
							onClick={() => {
								setSql("");
								setOutcomes([]);
								setExecutedAt(null);
							}}
						>
							<Eraser />
							清空
						</Button>
						<Button
							variant="destructive"
							size="sm"
							disabled={running || statements.length === 0}
							onClick={() => void execute()}
						>
							<Play />
							{running && progress ? `执行中 ${progress.done}/${progress.total}` : "执行"}
						</Button>
					</Toolbar>
				}
			/>

			<div className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm">
				<ShieldAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
				<div className="space-y-1">
					<p className="font-medium text-destructive">这是生产数据库的直接执行入口</p>
					<p className="text-muted-foreground">
						写语句不可撤销；执行前需要输入确认词{" "}
						<span className="font-mono">{EXECUTE_PHRASE}</span>。多语句会在前端按 `;`
						切分后<strong>逐条</strong>提交（后端禁止单请求多语句）；写语句会自动补上{" "}
						<span className="font-mono">{ADMIN_CONFIRMED_PREFIX}</span> 前缀。
					</p>
				</div>
			</div>

			<SectionCard
				title="语句"
				description={`已识别 ${statements.length} 条语句${writeCount > 0 ? `（其中 ${writeCount} 条为写语句）` : ""}；单条上限 ${MAX_SQL_LENGTH} 字符，超时 ${SQL_TIMEOUT_SECS}s。`}
			>
				<div className="space-y-3">
					<Textarea
						value={sql}
						onChange={(event) => setSql(event.target.value)}
						placeholder="SELECT * FROM users LIMIT 10;"
						className="min-h-56 font-mono text-sm"
						aria-label="SQL 输入"
					/>
					<div className="space-y-1">
						<p className="text-xs text-muted-foreground">常用查询示例（点击填入）</p>
						<div className="flex flex-col gap-1">
							{EXAMPLES.map((example) => (
								<button
									key={example}
									type="button"
									className="rounded border bg-muted/30 px-2 py-1 text-left font-mono text-xs hover:bg-muted"
									onClick={() => setSql(example)}
								>
									{example}
								</button>
							))}
						</div>
					</div>
				</div>
			</SectionCard>

			<SectionCard
				title="执行结果"
				description={
					executedAt
						? `最近一次执行：${new Date(executedAt).toLocaleString("zh-CN", { hour12: false })}`
						: "执行后在此逐条显示结果 / 后端错误。"
				}
				actions={
					running && progress ? (
						<TonePill tone="info">
							执行中 {progress.done}/{progress.total}
						</TonePill>
					) : null
				}
			>
				{outcomes.length === 0 ? (
					<EmptyBlock
						title="还没有执行结果"
						description="输入 SQL 并点击执行；每条语句的结果与错误会分别列出。"
					/>
				) : (
					<ul className="space-y-3">
						{outcomes.map((outcome, index) => (
							<li key={`${index}-${outcome.sql.slice(0, 24)}`} className="space-y-2 rounded-lg border p-3">
								<div className="flex flex-wrap items-center gap-2">
									<TonePill tone={outcome.ok ? "success" : "danger"}>
										#{index + 1} {outcome.ok ? "成功" : "失败"}
									</TonePill>
									{outcome.result ? (
										<>
											<TonePill tone="muted">{outcome.result.sql_type}</TonePill>
											<span className="tnum font-mono text-xs text-muted-foreground">
												{outcome.result.sql_type === "query"
													? `${formatInt(outcome.result.count)} 行`
													: `影响 ${formatInt(outcome.result.rows_affected)} 行`}{" "}
												· {formatInt(Number(outcome.result.elapsed_ms))} ms
											</span>
										</>
									) : null}
								</div>
								<ReadonlyBlock className="font-mono text-xs">{outcome.sql}</ReadonlyBlock>
								{outcome.errorMessage ? (
									<div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2">
										<AlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" />
										<p className="text-sm break-words text-destructive">
											{outcome.errorMessage}
										</p>
									</div>
								) : null}
								{outcome.result && outcome.result.rows.length > 0 ? (
									<ResultTable rows={outcome.result.rows} />
								) : null}
							</li>
						))}
					</ul>
				)}
			</SectionCard>
		</PageBody>
	);
}

/** 动态列的查询结果表（列取自第一行；值为 JSON 时序列化展示）。 */
function ResultTable({ rows }: { rows: Array<Record<string, unknown>> }): ReactNode {
	const columns: DataTableColumn<Record<string, unknown>>[] = Object.keys(rows[0] ?? {}).map(
		(key) => ({
			id: key,
			header: key,
			cell: (row) => <MonoText>{truncate(cellText(row[key]), 200)}</MonoText>,
		}),
	);

	return (
		<div className="rounded-md border">
			<DataTable
				data={rows}
				getRowId={(_row, index) => String(index)}
				columns={columns}
				empty={<span className="text-sm text-muted-foreground">查询没有返回数据行</span>}
			/>
			<p className="border-t px-3 py-2 text-xs text-muted-foreground">
				共 {formatInt(rows.length)} 行
			</p>
		</div>
	);
}
