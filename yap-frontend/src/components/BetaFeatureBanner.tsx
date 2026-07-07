import type { ReactNode } from "react";
import { Badge } from "@/components/ui/badge";

/**
 * Header banner for experimental tools. Drop it at the top of any beta
 * tool page; pass children to override the default explainer text.
 */
export function BetaFeatureBanner({ children }: { children?: ReactNode }) {
  return (
    <div className="flex items-center gap-2">
      <Badge variant="secondary">🧪 Beta</Badge>
      <span className="text-sm text-muted-foreground">
        {children ??
          "This feature isn't ready for prime time yet, but you can test it here if you want."}
      </span>
    </div>
  );
}
