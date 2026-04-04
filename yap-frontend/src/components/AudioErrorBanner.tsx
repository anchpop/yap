import { WifiOff } from "lucide-react";
import { Button } from "@/components/ui/button";

interface AudioErrorBannerProps {
  onSkip: () => void;
  label?: string;
}

export function AudioErrorBanner({
  onSkip,
  label = "Skip for now",
}: AudioErrorBannerProps) {
  return (
    <div className="rounded-lg p-4 border bg-orange-500/10 border-orange-500/20 flex flex-col items-center gap-3 animate-fade-in">
      <div className="flex items-center gap-2 text-orange-600 dark:text-orange-400">
        <WifiOff className="h-4 w-4" />
        <p className="text-sm font-medium">
          Audio unavailable — you may be offline
        </p>
      </div>
      <Button onClick={onSkip} variant="outline" size="sm">
        {label}
      </Button>
    </div>
  );
}
