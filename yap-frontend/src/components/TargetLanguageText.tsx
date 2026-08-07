import type { ReactNode } from "react";
import type { Language } from "../../../yap-frontend-rs/pkg";
import { languageToLangAttr } from "@/lib/utils";

export function TargetLanguageText({
  children,
  language,
}: {
  children: ReactNode;
  language: Language;
}) {
  return <span lang={languageToLangAttr(language)}>{children}</span>;
}
