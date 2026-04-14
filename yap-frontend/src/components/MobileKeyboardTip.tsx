import { useState, useEffect } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { type Language } from "../../../yap-frontend-rs/pkg/yap_frontend_rs";
import { match } from "ts-pattern";

interface MobileKeyboardTipProps {
  language: Language;
  className?: string;
}

const DISMISS_KEY = "mobile-keyboard-tip-dismissed";

export function MobileKeyboardTip({
  language,
  className = "",
}: MobileKeyboardTipProps) {
  const [isDismissed, setIsDismissed] = useState(false);

  useEffect(() => {
    // Check if tip has been dismissed before
    const dismissed = localStorage.getItem(DISMISS_KEY) === "true";
    setIsDismissed(dismissed);
  }, []);

  const handleDismiss = () => {
    setIsDismissed(true);
    localStorage.setItem(DISMISS_KEY, "true");
  };

  if (isDismissed) {
    return null;
  }

  const characterType = match(language)
    .with("French", () => "accented")
    .with("Spanish", () => "accented")
    .with("German", () => "accented")
    .with("Korean", () => "hangul")
    .with("English", () => null)
    .with("Chinese", () => "Chinese")
    .with("Japanese", () => "Japanese")
    .with("Russian", () => "Cyrillic")
    .with("Portuguese", () => "accented")
    .with("Italian", () => "accented")
    .with("Hindi", () => "Devanagari")
    .exhaustive();

  // Don't show tip for English
  if (!characterType) {
    return null;
  }

  return (
    <div
      className={`md:hidden flex items-center justify-between gap-2 p-3 mt-3 border rounded-lg bg-muted/30 ${className}`}
    >
      <p className="text-sm text-muted-foreground flex-1">
        <span className="font-medium">Tip:</span> Enable the {language} keyboard
        on your device to easily type {characterType} characters
      </p>
      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 shrink-0"
        onClick={handleDismiss}
      >
        <X className="h-4 w-4" />
      </Button>
    </div>
  );
}
