import type { ReactNode } from "react";
import type { Language } from "../../../yap-frontend-rs/pkg";
import { languageToBcp47 } from "@/lib/utils";

export function TargetLanguageText({
  children,
  language,
}: {
  children: ReactNode;
  language: Language;
}) {
  return <span lang={languageToBcp47(language)}>{children}</span>;
}
