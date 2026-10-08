/**
 * 管理端核心域的页面出口（路由层只从这里取组件）。
 *
 * 页面拆分：
 * - `dashboard-page.tsx`          总览（聚合 + 系统监控 + 版本）
 * - `users-page.tsx`              用户管理
 * - `challenges-page.tsx`         题库管理（CRUD + 导入/校验/构建/扫描）
 * - `challenge-sets-page.tsx`     题集管理（列表）
 * - `challenge-set-detail-page.tsx` 题集详情（题目增删）
 * - `gameboxes-page.tsx`          GameBox 库管理
 * - `network-page.tsx`            靶场网络（设置 / 健康 / 分配）
 */

export { AdminDashboardPage } from "./dashboard-page";
export { AdminUsersPage } from "./users-page";
export { AdminChallengesPage } from "./challenges-page";
export { AdminChallengeSetsPage } from "./challenge-sets-page";
export { AdminChallengeSetDetailPage } from "./challenge-set-detail-page";
export { AdminGameboxesPage } from "./gameboxes-page";
export { AdminNetworkPage } from "./network-page";
