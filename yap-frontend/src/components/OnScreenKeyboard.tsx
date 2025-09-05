import { useCallback, useState } from "react";
import Keyboard from "react-simple-keyboard";
import "react-simple-keyboard/build/css/index.css";
import "./OnScreenKeyboard.css";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Keyboard as KeyboardIcon } from "lucide-react";
import koreanLayout from "simple-keyboard-layouts/build/layouts/korean";
import type { Language } from "../../../yap-frontend-rs/pkg/yap_frontend_rs";

interface OnScreenKeyboardProps {
  language: Language;
}

type KeyboardLayout = {
  default: string[];
  shift?: string[];
};

export function OnScreenKeyboard({ language }: OnScreenKeyboardProps) {
  const layouts: Partial<Record<Language, KeyboardLayout>> = {
    French: {
      default: [
        "à â æ ç é è ê ë",
        "î ï ô œ ù û ü ÿ",
        "{space} {bksp}",
      ],
    },
    Spanish: {
      default: [
        "á é í ó ú ü ñ",
        "¡ ¿",
        "{space} {bksp}",
      ],
    },
    Korean: koreanLayout as unknown as KeyboardLayout,
  };

  const [layoutName, setLayoutName] = useState<"default" | "shift">("default");

  const handleKeyPress = useCallback(
    (button: string) => {
      if (button === "{shift}" || button === "{lock}") {
        setLayoutName((name) => (name === "default" ? "shift" : "default"));
        return;
      }

      const active = document.activeElement as HTMLInputElement | null;
      if (!active) return;
      if (button === "{bksp}") {
        const start = active.selectionStart ?? 0;
        const end = active.selectionEnd ?? 0;
        if (start === end) {
          active.setRangeText("", Math.max(start - 1, 0), start, "end");
        } else {
          active.setRangeText("", start, end, "end");
        }
      } else if (button === "{space}") {
        const pos = active.selectionStart ?? 0;
        active.setRangeText(" ", pos, pos, "end");
      } else {
        const pos = active.selectionStart ?? 0;
        active.setRangeText(button, pos, pos, "end");
      }
      active.dispatchEvent(new Event("input", { bubbles: true }));
      active.focus();
    },
    []
  );

  const layout = layouts[language];
  if (!layout) return null;

  return (
    <div className="hidden [@media(hover:hover)_and_(pointer:fine)]:flex flex-col items-center mt-2">
      <Collapsible>
        <CollapsibleTrigger asChild>
          <Button variant="outline" size="icon" className="mb-2">
            <KeyboardIcon className="h-4 w-4" />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <Keyboard
            layout={layout}
            layoutName={layoutName}
            onKeyPress={handleKeyPress}
            display={{ "{bksp}": "⌫" }}
          />
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}

