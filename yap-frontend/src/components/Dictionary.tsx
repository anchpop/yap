import { useState, useEffect } from 'react'
import { type Deck, type Weapon, type Language, type Heteronym, get_word_prefix } from '../../../yap-frontend-rs/pkg'
import { CirclePlus, CircleCheckBig } from 'lucide-react'
import { toast } from 'sonner'
import { formatMorphology } from '@/utils/formatMorphology'
import { Card } from '@/components/ui/card'

// Helper function to get language display name (exhaustive)
function getLanguageDisplayName(language: Language): string {
  switch (language) {
    case 'French':
      return 'French'
    case 'English':
      return 'English'
    case 'Spanish':
      return 'Spanish'
    case 'Korean':
      return 'Korean'
    case 'German':
      return 'German'
    case 'Chinese':
      return 'Chinese'
    case 'Japanese':
      return 'Japanese'
    case 'Russian':
      return 'Russian'
    case 'Portuguese':
      return 'Portuguese'
    case 'Italian':
      return 'Italian'
    default: {
      // TypeScript will error if we add a new language and don't handle it
      const exhaustiveCheck: never = language
      throw new Error(`Unhandled language: ${exhaustiveCheck}`)
    }
  }
}

// Helper function to get language code (exhaustive)
function getLanguageCode(language: Language): string {
  switch (language) {
    case 'French':
      return 'FR'
    case 'English':
      return 'EN'
    case 'Spanish':
      return 'ES'
    case 'Korean':
      return 'KO'
    case 'German':
      return 'DE'
    case 'Chinese':
      return 'ZH'
    case 'Japanese':
      return 'JA'
    case 'Russian':
      return 'RU'
    case 'Portuguese':
      return 'PT'
    case 'Italian':
      return 'IT'
    default: {
      const exhaustiveCheck: never = language
      throw new Error(`Unhandled language: ${exhaustiveCheck}`)
    }
  }
}

// Helper function to remove accents from a string for accent-insensitive search
function removeAccents(str: string): string {
  return str.normalize('NFD').replace(/[\u0300-\u036f]/g, '')
}

export function Dictionary({ deck, weapon, targetLanguage, nativeLanguage }: { deck: Deck, weapon: Weapon, targetLanguage: Language, nativeLanguage: Language }) {
  const [searchQuery, setSearchQuery] = useState('')
  const [addedCards, setAddedCards] = useState<Set<string>>(new Set())

  const entries = deck.get_dictionary_entries()

  // Get language codes and display names for UI
  const targetLangCode = getLanguageCode(targetLanguage)
  const nativeLangCode = getLanguageCode(nativeLanguage)
  const targetLangName = getLanguageDisplayName(targetLanguage)
  const nativeLangName = getLanguageDisplayName(nativeLanguage)

  // Check which cards are already added
  useEffect(() => {
    const allCards = deck.get_all_cards_summary()
    const added = new Set<string>()
    allCards.forEach(card => {
      if (card.card_indicator.TargetLanguage) {
        const lexeme = card.card_indicator.TargetLanguage.lexeme
        if (lexeme.Heteronym) {
          const key = `${lexeme.Heteronym.word}-${lexeme.Heteronym.lemma}-${lexeme.Heteronym.pos}`
          added.add(key)
        }
      }
    })
    setAddedCards(added)
  }, [deck])

  const handleAddCard = (word: string, heteronym: Heteronym<string>) => {
    // Create the card indicator
    const cardIndicator = {
      TargetLanguage: {
        lexeme: {
          Heteronym: heteronym
        }
      }
    }

    // Create the AddCards event
    const event = {
      Language: {
        target_language: targetLanguage,
        content: {
          AddCards: {
            cards: [cardIndicator]
          }
        }
      }
    }

    // Add the event to the weapon
    weapon.add_deck_event(event)

    // Update the added cards set
    const key = `${heteronym.word}-${heteronym.lemma}-${heteronym.pos}`
    setAddedCards(prev => new Set(prev).add(key))
    toast.success(`Added "${word}" to your deck`)
  }

  const isCardAdded = (heteronym: any) => {
    const key = `${heteronym.word}-${heteronym.lemma}-${heteronym.pos}`
    return addedCards.has(key)
  }

  const filteredEntries = (() => {
    if (!searchQuery.trim()) return entries.slice(0, 100) // Only show top 100 when not searching

    const query = removeAccents(searchQuery.toLowerCase())
    return entries.filter(entry => {
      // Search in target language word (accent-insensitive)
      if (removeAccents(entry.word.toLowerCase()).includes(query)) return true

      // Search in native language translations (accent-insensitive)
      return entry.entry.definitions.some(def =>
        removeAccents(def.native.toLowerCase()).includes(query)
      )
    }).slice(0, 100) // Limit search results to 100 as well
  })()

  return (
    <div className="flex-1 overflow-hidden flex flex-col">
      <div className="border-b pb-4 mb-4 p-2">
        <input
          type="text"
          placeholder={`Search in ${targetLangName} or ${nativeLangName}...`}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="w-full px-4 py-2 border rounded-lg bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
        />
        <p className="text-sm text-muted-foreground mt-2">
          Showing {filteredEntries.length} of {entries.length} {entries.length === 1 ? 'entry' : 'entries'}
          {searchQuery && ` matching "${searchQuery}"`}
        </p>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        <div className="space-y-4">
          {filteredEntries.map((entry, index) => {
            const prefix = get_word_prefix(
              entry.entry.morphology[0],
              entry.word,
              entry.heteronym.pos,
              targetLanguage
            )

            return (
            <Card key={`${entry.word}-${index}`} className="p-4 relative gap-0">
              <div className="flex items-baseline justify-between gap-4 mb-2">
                <h2 className="text-xl font-semibold">
                  {prefix && (
                    <span className="text-muted-foreground/60">
                      {prefix.prefix}{prefix.separator}
                    </span>
                  )}
                  {entry.word}
                </h2>
                {(() => {
                  const morphologyText = entry.entry.morphology[0] ? formatMorphology(entry.entry.morphology[0]) : ''
                  return morphologyText && (
                    <span className="text-sm text-muted-foreground italic">
                      {morphologyText}
                    </span>
                  )
                })()}
              </div>

              <div className="space-y-3">
                {entry.entry.definitions.map((def, defIndex) => (
                  <div key={defIndex} className="pl-4 border-l-2 border-muted">
                    <div className="font-medium text-primary">{def.native}</div>
                    {def.note && (
                      <div className="text-sm text-muted-foreground italic mt-1">
                        {def.note}
                      </div>
                    )}
                    <div className="mt-2 text-sm space-y-1">
                      <div className="text-foreground">
                        <span className="text-muted-foreground">{targetLangCode}:</span> {def.example_sentence_target_language}
                      </div>
                      <div className="text-muted-foreground">
                        <span>{nativeLangCode}:</span> {def.example_sentence_native_language}
                      </div>
                    </div>
                  </div>
                ))}
              </div>

              <div className="absolute bottom-3 right-3">
                {isCardAdded(entry.heteronym) ? (
                  <button
                    disabled
                    className="flex items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground cursor-default"
                  >
                    <CircleCheckBig className="w-4 h-4" />
                    <span>Added</span>
                  </button>
                ) : (
                  <button
                    onClick={() => handleAddCard(entry.word, entry.heteronym)}
                    className="flex items-center gap-2 px-3 py-1.5 text-sm text-foreground hover:bg-muted rounded-md transition-colors"
                  >
                    <CirclePlus className="w-4 h-4" />
                    <span>Add to deck</span>
                  </button>
                )}
              </div>
            </Card>
            )
          })}

          {filteredEntries.length === 0 && (
            <div className="text-center py-12 text-muted-foreground">
              {searchQuery ? 'No entries found matching your search.' : 'No dictionary entries available.'}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
