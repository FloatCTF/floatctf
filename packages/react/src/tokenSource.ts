/**
 * React binding 的令牌来源抽象。
 *
 * `@floatctf/react` **不**假设任何认证状态实现：前端把自己的 hook（Zustand selector、
 * Context selector、内存订阅……）传进来即可。这样 headless 绑定永远不会把
 * 某个状态库变成平台依赖。
 */
export type UseTokenSource = () => string | null | undefined;
