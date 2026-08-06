import { InlineMessage } from "@primer/react/experimental";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";

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
  const showErrorBanner = (error: unknown) => {
    const msg =
      (error as AxiosError<{ message: string }>)?.response?.data?.message ||
      (error as Error).message ||
      "Unknown error";
    showBanner("critical", msg);
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
