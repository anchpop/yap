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
    <div className={`accent-keyboard flex flex-wrap justify-center ${className}`}>
      {characters.map((char, index) => (
        <Button
          key={char}
          variant="outline"
          size="sm"
          className={`h-8 w-10 text-base font-medium rounded-none border-r-0 last:border-r ${
            index === 0 ? 'rounded-l-md' : ''
          } ${
            index === characters.length - 1 ? 'rounded-r-md' : ''
          }`}
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
