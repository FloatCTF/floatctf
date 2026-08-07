import { defineConfig } from "vite";
import viteReact from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import pkg from "./package.json";
import { TanStackRouterVite } from "@tanstack/router-plugin/vite";
import { resolve } from "node:path";

// https://vitejs.dev/config/
export default defineConfig({
    define: {
        "import.meta.env.VITE_APP_VERSION": JSON.stringify(pkg.version),
    },
    plugins: [
        TanStackRouterVite({ autoCodeSplitting: true }),
        viteReact(),
        tailwindcss(),
    ],

    resolve: {
        alias: {
            "@": resolve(__dirname, "./src"),
        },
    },
    server: {
        // host: true 让 dev server 监听 0.0.0.0，
        // 这样 nginx 容器可通过 host-gateway (172.17.0.1:3000) 反向代理。
        host: true,
        watch: {
            ignored: ["**/routeTree.gen.ts"], // ← 加这 3 行
        },
    },
});
