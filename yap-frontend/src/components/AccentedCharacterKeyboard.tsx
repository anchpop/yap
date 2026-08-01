import { Button } from "@/components/ui/button";
import { type Language } from "../../../yap-frontend-rs/pkg/yap_frontend_rs";
import { match } from "ts-pattern";
import { Card } from "./ui/card";

interface AccentedCharacterKeyboardProps {
  onCharacterInsert: (char: string) => void;
  language: Language;
  className?: string;
  uppercase?: boolean;
}

export function AccentedCharacterKeyboard({
  onCharacterInsert,
  language,
  className = "",
  uppercase = false,
}: AccentedCharacterKeyboardProps) {
  const baseCharacters = match(language)
    .with("French", () => [
      "à",
      "â",
      "é",
      "è",
      "ê",
      "ë",
      "î",
      "ï",
      "ô",
      "ù",
      "û",
      "ü",
      "ÿ",
      "ç",
      "œ",
      "æ",
    ])
    .with("Spanish", () => ["á", "é", "í", "ó", "ú", "ü", "ñ", "¿", "¡"])
    .with("German", () => ["ä", "ö", "ü", "ß", "Ä", "Ö", "Ü"])
    .with("Korean", () => [])
    .with("English", () => [])
    .with("ChineseSimplified", () => [])
    .with("ChineseTraditional", () => [])
    .with("Japanese", () => [])
    .with("Russian", () => [])
    .with("Portuguese", () => [
      "á",
      "é",
      "í",
      "ó",
      "ú",
      "â",
      "ê",
      "ô",
      "ã",
      "õ",
      "ç",
    ])
    .with("Italian", () => ["à", "è", "é", "ì", "ò", "ù"])
    .with("Hindi", () => [])
    .exhaustive();

  const characters = uppercase
    ? [...new Set(baseCharacters.map((c) => c.toUpperCase()))]
    : baseCharacters;

  // Split characters into rows of at most 8 characters each
  const rows: string[][] = [];
  for (let i = 0; i < characters.length; i += 8) {
    rows.push(characters.slice(i, i + 8));
  }
  if (characters.length === 0) {
    return null;
  }

  return (
    <Card
      variant="light"
      className={`accent-keyboard flex flex-col items-center ${className} gap-0`}
    >
      {rows.map((row, rowIndex) => (
        <div key={rowIndex} className="flex justify-center">
          {row.map((char, index) => (
            <Button
              key={char}
              variant="ghost"
              size="sm"
              className={`h-8 w-10 text-base font-medium rounded-none border-r-0 ${
                index === 0 ? "rounded-l-md" : ""
              } ${index === row.length - 1 ? "rounded-r-md" : ""}`}
              onClick={() => onCharacterInsert(char)}
              onMouseDown={(e) => e.preventDefault()}
              type="button"
            >
              {char}
            </Button>
          ))}
        </div>
      ))}
    </Card>
  );
}
