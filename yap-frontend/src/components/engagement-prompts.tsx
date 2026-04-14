import { memo, useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Bell, Sparkles } from "lucide-react";
import { useOneSignalNotifications } from "@/hooks/use-onesignal-notifications";
import { useIsInstalled } from "@/hooks/use-is-installed";
import { InstallPwaButton } from "@/components/InstallPwaButton";
import { Card } from "@/components/ui/card";
import { match } from "ts-pattern";
import type { Language } from "../../../yap-frontend-rs/pkg";

interface EngagementPromptsProps {
  language: Language;
}

export const EngagementPrompts = memo(function EngagementPrompts({ language }: EngagementPromptsProps) {
  const {
    isSupported,
    isSubscribed,
    isLoading: isNotificationLoading,
    isInitialized,
    subscribe,
  } = useOneSignalNotifications();

  const { isInstalled, isLoading: isInstalledLoading } = useIsInstalled();
  const [promptsDismissed, setPromptsDismissed] = useState(false);

  useEffect(() => {
    // Check dismissal count first
    const dismissalCount = parseInt(
      localStorage.getItem("engagement-prompts-dismissal-count") || "0",
      10,
    );

    // If dismissed 3 or more times, never show again
    if (dismissalCount >= 3) {
      setPromptsDismissed(true);
      return;
    }

    // Check if user has previously dismissed the engagement prompts
    const dismissedTime = localStorage.getItem("engagement-prompts-dismissed");
    if (dismissedTime) {
      const dismissedTimestamp = parseInt(dismissedTime, 10);
      const now = Date.now();
      const oneDayInMs = 24 * 60 * 60 * 1000;

      // If less than 24 hours have passed, keep it dismissed
      if (now - dismissedTimestamp < oneDayInMs) {
        setPromptsDismissed(true);
      } else {
        // More than 24 hours have passed, remove the dismissal
        localStorage.removeItem("engagement-prompts-dismissed");
      }
    }
  }, []);

  const handleDismiss = () => {
    setPromptsDismissed(true);

    // Increment dismissal count
    const currentCount = parseInt(
      localStorage.getItem("engagement-prompts-dismissal-count") || "0",
      10,
    );
    const newCount = currentCount + 1;
    localStorage.setItem(
      "engagement-prompts-dismissal-count",
      newCount.toString(),
    );

    // Only set the timestamp if we haven't hit the permanent dismissal threshold
    if (newCount < 3) {
      const now = Date.now();
      localStorage.setItem("engagement-prompts-dismissed", now.toString());
    }
  };

  // Calculate what to show
  const shouldShowAddToHomeScreen = !isInstalledLoading && !isInstalled;
  const shouldShowNotifications = isInitialized && isSupported && !isSubscribed;
  const shouldShowAnything =
    (shouldShowAddToHomeScreen || shouldShowNotifications) && !promptsDismissed;

  if (!shouldShowAnything) {
    return null;
  }

  const headingText = match(language)
    .with("French", () => "Stay on track with your French learning")
    .with("Spanish", () => "Stay on track with your Spanish learning")
    .with("Korean", () => "Stay on track with your Korean learning")
    .with("English", () => "Stay on track with your English learning")
    .with("German", () => "Stay on track with your German learning")
    .with("Chinese", () => "Stay on track with your Chinese learning")
    .with("Japanese", () => "Stay on track with your Japanese learning")
    .with("Russian", () => "Stay on track with your Russian learning")
    .with("Portuguese", () => "Stay on track with your Portuguese learning")
    .with("Italian", () => "Stay on track with your Italian learning")
    .with("Hindi", () => "Stay on track with your Hindi learning")
    .exhaustive();

  return (
    <Card variant="light" animate className="gap-0 px-3 py-4 sm:p-6">
      <div className="flex items-center gap-2 mb-3 sm:mb-4">
        <Sparkles className="h-5 w-5 text-primary shrink-0" />
        <h3 className="font-semibold text-sm sm:text-base">{headingText}</h3>
      </div>

      <p className="text-xs sm:text-sm text-muted-foreground mb-3 sm:mb-4">
        Research shows that consistent daily practice is key to language
        learning success. These features help you maintain your streak:
      </p>

      <div className="flex flex-col gap-3 sm:grid sm:grid-cols-[auto_1fr]">
        {shouldShowAddToHomeScreen && (
          <>
            <InstallPwaButton
              variant="outline"
              size="sm"
              className="justify-start"
            />
            <p className="text-xs text-muted-foreground self-center">
              {window.navigator.userAgent.match(/mobile/i)
                ? "Quick access from your home screen makes it easier to practice daily"
                : "Install as a desktop app for quick access and offline use"}
            </p>
          </>
        )}

        {shouldShowNotifications && (
          <>
            <Button
              onClick={subscribe}
              disabled={isNotificationLoading}
              variant="outline"
              size="sm"
              className="justify-start"
            >
              <Bell className="mr-2 h-4 w-4" />
              {isNotificationLoading ? "Enabling..." : "Enable Reminders"}
            </Button>
            <p className="text-xs text-muted-foreground self-center">
              Get gentle reminders when you have cards ready to review
            </p>
          </>
        )}
      </div>

      <div className="flex justify-end mt-3 sm:mt-4">
        <Button
          onClick={handleDismiss}
          variant="ghost"
          size="sm"
          className="text-xs"
        >
          Maybe later
        </Button>
      </div>
    </Card>
  );
});
