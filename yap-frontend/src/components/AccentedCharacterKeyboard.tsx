import { Button } from "@/components/ui/button"
import { type Language } from '../../../yap-frontend-rs/pkg/yap_frontend_rs'

interface AccentedCharacterKeyboardProps {
  onCharacterInsert: (char: string) => void
  language: Language
  className?: string
}

const accentedCharacters: Record<string, string[]> = {
  French: ['à', 'â', 'é', 'è', 'ê', 'ë', 'î', 'ï', 'ô', 'ù', 'û', 'ü', 'ÿ', 'ç', 'œ', 'æ'],
  Spanish: ['á', 'é', 'í', 'ó', 'ú', 'ü', 'ñ', '¿', '¡'],
}

export function AccentedCharacterKeyboard({ 
  onCharacterInsert, 
  language,
  className = ""
}: AccentedCharacterKeyboardProps) {
  const characters = accentedCharacters[language] || []
  
  if (characters.length === 0) {
    return null
  }

  return (
    <div className={`accent-keyboard flex flex-wrap gap-1 justify-center ${className}`}>
      {characters.map((char) => (
        <Button
          key={char}
          variant="outline"
          size="sm"
          className="h-8 w-10 text-base font-medium"
          onClick={() => onCharacterInsert(char)}
          onMouseDown={(e) => e.preventDefault()}
          type="button"
        >
          {char}
        </Button>
      ))}
    </div>
  )
}