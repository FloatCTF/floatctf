/**
 * dev 入口：把本前端挂到 `#root`。
 *
 * 刻意复用**同一个** `mountApp`：如果 dev 与生产走两条装配路径，
 * 「dev 能跑、装上去白屏」这类问题就会在最后一刻才暴露。
 */
import { mount } from "./entry";

const root = document.getElementById("root");
if (!root) throw new Error("dev 入口缺少 #root");

mount({
	root,
	apiBaseUrl: "/api",
	assetBaseUrl: "/",
	frontendId: "animal-island",
	frontendVersion: "0.1.0",
	platformVersion: "dev",
	apiContractVersion: "2",
	frontendRuntimeVersion: "1",
	capabilities: ["jeopardy", "awd", "awdp", "discussions", "writeups", "web_terminal"],
});
