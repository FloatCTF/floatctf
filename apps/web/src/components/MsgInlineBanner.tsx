import { InlineMessage } from "@primer/react/experimental";
import { useReactive } from "ahooks";

import { formatApiError } from "./apiErrorMessage";

export type MessageVariant = "critical" | "success" | "unavailable" | "warning";

export interface UseMsgInlineBannerOptions {
  isShown?: boolean; // 是否默认显示
  message?: string; // 默认描述
  variant?: MessageVariant; // 默认类型
}
export const useMsgInlineBanner = (options: UseMsgInlineBannerOptions = {}) => {
  const mutationBanner = useReactive({
    isShown: options.isShown ?? false,
    message: options.message ?? "Something here",
    variant: options.variant ?? ("success" as MessageVariant),
  });
  const showBanner = (variant: MessageVariant, message: string) => {
    mutationBanner.isShown = true;
    mutationBanner.variant = variant;
    mutationBanner.message = message;
  };
  /** `overrides` 可按状态码覆盖文案（例如登录页把 401 解释为账号密码错误）。 */
  const showErrorBanner = (
    error: unknown,
    overrides?: Record<number, string>,
  ) => {
    showBanner("critical", formatApiError(error, overrides));
  };

  const hideBanner = () => {
    mutationBanner.isShown = false;
  };

  const BannerComponent = ({ className }: { className?: string }) =>
    mutationBanner.isShown ? (
      <InlineMessage variant={mutationBanner.variant} className={className}>
        {mutationBanner.message}
      </InlineMessage>
    ) : null;

  return {
    showBanner,
    showErrorBanner,
    hideBanner,
    BannerComponent,
  };
};
