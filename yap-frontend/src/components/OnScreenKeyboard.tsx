import { useCallback } from "react";
import Keyboard from "react-simple-keyboard";
import "react-simple-keyboard/build/css/index.css";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import type { Language } from "../../../yap-frontend-rs/pkg/yap_frontend_rs";

interface OnScreenKeyboardProps {
  language: Language;
}

export function OnScreenKeyboard({ language }: OnScreenKeyboardProps) {
  const layouts: Partial<Record<Language, { default: string[] }>> = {
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
    Korean: {
      default: [
        "ㅂ ㅈ ㄷ ㄱ ㅅ ㅛ ㅕ ㅑ ㅐ ㅔ",
        "ㅁ ㄴ ㅇ ㄹ ㅎ ㅗ ㅓ ㅏ ㅣ",
        "ㅋ ㅌ ㅊ ㅍ ㅠ ㅜ ㅡ",
        "{space} {bksp}",
      ],
    },
  };

  const handleKeyPress = useCallback((button: string) => {
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
  }, []);

  const layout = layouts[language];
  if (!layout) return null;

  return (
    <div className="hidden [@media(hover:hover)_and_(pointer:fine)]:block">
      <Collapsible>
        <CollapsibleTrigger asChild>
          <Button variant="outline" size="sm" className="mb-2">
            Keyboard
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <Keyboard layout={layout} onKeyPress={handleKeyPress} display={{ "{bksp}": "⌫" }} />
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}

