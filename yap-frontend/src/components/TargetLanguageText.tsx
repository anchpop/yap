import type { ReactNode } from "react";
import type { Language } from "../../../yap-frontend-rs/pkg";
import { languageToIso6391 } from "@/lib/utils";

export function TargetLanguageText({ children, language }: { children: ReactNode; language: Language }) {
  return <span lang={languageToIso6391(language)}>{children}</span>;
}
