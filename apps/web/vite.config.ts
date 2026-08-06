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
        watch: {
            ignored: ["**/routeTree.gen.ts"], // ← 加这 3 行
        },
    },
});
