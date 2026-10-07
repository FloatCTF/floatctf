/**
 * 极端兜底 UI：当**连默认前端都加载失败**时，bootstrap 用纯 DOM 渲染这一段。
 *
 * 约束：
 * - 不依赖 React / Primer / Tailwind / 任何外部资源
 * - 只展示诊断信息（版本、ID、错误文本），**不含任何密钥、token、路径或后端内部信息**
 * - 只使用 `textContent`（绝不用 `innerHTML` 拼接错误文本，避免把外部输入当 HTML 执行）
 */

export interface BootstrapDiagnostics {
	/** 平台版本（来自 `/api/frontend`，可能取不到）。 */
	platformVersion?: string;
	apiContractVersion?: string;
	frontendRuntimeVersion?: string;
	/** 平台设置里的 FRONTEND_ACTIVE。 */
	activeFrontendId?: string | null;
	/** `?frontend=` 覆盖值（若有效）。 */
	overrideFrontendId?: string | null;
	/** 本次实际尝试加载过的前端 ID（按顺序）。 */
	attempted: string[];
	/** 每次尝试的失败原因（与 `attempted` 对位）。 */
	errors: string[];
}

/**
 * 渲染兜底错误页；返回容器元素。
 *
 * @param host 挂载点（通常是 `document.body`）
 * @param diagnostics 诊断信息
 * @param onRetry 可选：点击"重试"时的回调（默认 `location.reload()`）
 */
export function renderBootstrapEmergencyUi(
	host: HTMLElement,
	diagnostics: BootstrapDiagnostics,
	onRetry?: () => void,
): HTMLElement {
	const doc = host.ownerDocument;
	host.textContent = "";

	const wrap = doc.createElement("div");
	wrap.setAttribute("data-floatctf-bootstrap-error", "true");
	wrap.style.cssText = [
		"max-width: 40rem",
		"margin: 4rem auto",
		"padding: 1.5rem",
		"font-family: ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif",
		"font-size: 15px",
		"line-height: 1.6",
		"color: #1f2328",
	].join(";");

	const title = doc.createElement("h1");
	title.style.cssText = "font-size: 1.25rem; margin: 0 0 .75rem;";
	title.textContent = "FloatCTF 界面加载失败";
	wrap.appendChild(title);

	const intro = doc.createElement("p");
	intro.style.cssText = "margin: 0 0 1rem;";
	intro.textContent =
		"当前选择的前端与内置默认前端都无法加载。请用 ?frontend=default 重试，或联系管理员检查前端安装。";
	wrap.appendChild(intro);

	const table = doc.createElement("table");
	table.style.cssText = "border-collapse: collapse; margin: 0 0 1rem; font-size: 13px;";
	const addRow = (label: string, value: string) => {
		const tr = doc.createElement("tr");
		const th = doc.createElement("th");
		th.style.cssText = "text-align: left; padding: .15rem .75rem .15rem 0; vertical-align: top;";
		th.textContent = label;
		const td = doc.createElement("td");
		td.style.cssText = "padding: .15rem 0; font-family: ui-monospace, SFMono-Regular, monospace;";
		td.textContent = value;
		tr.append(th, td);
		table.appendChild(tr);
	};

	addRow("平台版本", diagnostics.platformVersion ?? "(未知)");
	addRow("API 契约", diagnostics.apiContractVersion ?? "(未知)");
	addRow("前端运行时契约", diagnostics.frontendRuntimeVersion ?? "(未知)");
	addRow("FRONTEND_ACTIVE", diagnostics.activeFrontendId ?? "(未设置)");
	if (diagnostics.overrideFrontendId) {
		addRow("?frontend=", diagnostics.overrideFrontendId);
	}
	addRow("已尝试", diagnostics.attempted.length > 0 ? diagnostics.attempted.join(" → ") : "(无)");
	wrap.appendChild(table);

	if (diagnostics.errors.length > 0) {
		const errTitle = doc.createElement("p");
		errTitle.style.cssText = "margin: 0 0 .25rem; font-weight: 600;";
		errTitle.textContent = "失败原因";
		wrap.appendChild(errTitle);

		const list = doc.createElement("ul");
		list.style.cssText =
			"margin: 0 0 1rem; padding-left: 1.25rem; font-size: 13px; font-family: ui-monospace, SFMono-Regular, monospace;";
		for (const error of diagnostics.errors) {
			const li = doc.createElement("li");
			li.style.cssText = "margin-bottom: .25rem;";
			li.textContent = error;
			list.appendChild(li);
		}
		wrap.appendChild(list);
	}

	const retry = doc.createElement("button");
	retry.type = "button";
	retry.style.cssText = [
		"padding: .4rem .9rem",
		"border: 1px solid #1f2328",
		"border-radius: 6px",
		"background: #1f2328",
		"color: #fff",
		"font-size: 14px",
		"cursor: pointer",
	].join(";");
	retry.textContent = "重试";
	retry.addEventListener("click", () => {
		if (onRetry) onRetry();
		else host.ownerDocument.defaultView?.location.reload();
	});
	wrap.appendChild(retry);

	host.appendChild(wrap);
	return wrap;
}
