import { useState } from "react";
import {
  type Deck,
  type Weapon,
  type Language,
  type GramDictionaryEntry,
} from "../../../yap-frontend-rs/pkg";
import { CirclePlus, CircleCheckBig } from "lucide-react";
import { toast } from "sonner";
import { formatMorphology } from "@/utils/formatMorphology";
import { highlightTermInSentence } from "@/utils/highlightTermInSentence";
import { Card } from "@/components/ui/card";
import { LANGUAGES } from "@/lib/languages";
import { TargetLanguageText } from "./TargetLanguageText";
import { AudioButton } from "./AudioButton";

const RESULTS_LIMIT = 100;

export function Dictionary({
  deck,
  weapon,
  targetLanguage,
  nativeLanguage,
  accessToken,
}: {
  deck: Deck;
  weapon: Weapon;
  targetLanguage: Language;
  nativeLanguage: Language;
  accessToken: string | undefined;
}) {
  const [searchQuery, setSearchQuery] = useState("");
  const [justAdded, setJustAdded] = useState<Set<number>>(new Set());

  const totalCount = deck.get_gram_dictionary_count();
  const entries = deck.get_gram_dictionary_entries(
    searchQuery.trim() || undefined,
    RESULTS_LIMIT,
  );

  // Get language codes and display names for UI
  const { badge: targetLangCode, englishName: targetLangName } =
    LANGUAGES[targetLanguage];
  const { badge: nativeLangCode, englishName: nativeLangName } =
    LANGUAGES[nativeLanguage];

  const handleAddCard = (entry: GramDictionaryEntry) => {
    const event = deck.add_gram_by_frequency_index(entry.frequency_index);
    if (event) {
      weapon.add_deck_event(event);
      setJustAdded((prev) => new Set(prev).add(entry.frequency_index));
      toast.success(`Added "${entry.display_text}" to your deck`);
    }
  };

  const isCardAdded = (entry: GramDictionaryEntry) => {
    return entry.is_in_deck || justAdded.has(entry.frequency_index);
  };

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
          Showing {entries.length} of {totalCount}{" "}
          {totalCount === 1 ? "entry" : "entries"}
          {searchQuery && ` matching "${searchQuery}"`}
        </p>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        <div className="space-y-4">
          {entries.map((entry) => {
            const morphologyText = entry.morphology
              ? formatMorphology(entry.morphology)
              : "";

            return (
              <Card key={entry.frequency_index} className="p-4 relative gap-0">
                <div className="flex items-baseline justify-between gap-4 mb-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <AudioButton
                      audioRequest={entry.audio_request}
                      accessToken={accessToken}
                      className="h-8 w-8 shrink-0"
                    />
                    <h2 className="text-xl font-semibold">
                      <TargetLanguageText language={targetLanguage}>
                        {entry.prefix && (
                          <span className="text-muted-foreground/60">
                            {entry.prefix.prefix}
                            {entry.prefix.separator}
                          </span>
                        )}
                        {entry.display_text}
                      </TargetLanguageText>
                      {entry.is_phrase && (
                        <span className="text-sm text-muted-foreground/60 font-normal ml-2">
                          (phrase)
                        </span>
                      )}
                    </h2>
                  </div>
                  {morphologyText && (
                    <span className="text-sm text-muted-foreground italic">
                      {morphologyText}
                    </span>
                  )}
                </div>

                <div className="space-y-3">
                  {"Dictionary" in entry.definition ? (
                    entry.definition.Dictionary.definitions.map(
                      (def, defIndex) => (
                        <div
                          key={defIndex}
                          className="pl-4 border-l-2 border-muted"
                        >
                          <div className="font-medium text-primary">
                            {def.native}
                          </div>
                          {def.note && (
                            <div className="text-sm text-muted-foreground italic mt-1">
                              {def.note}
                            </div>
                          )}
                          <div className="mt-2 text-sm space-y-1">
                            <div className="text-foreground">
                              <span className="text-muted-foreground">
                                {targetLangCode}:
                              </span>{" "}
                              <TargetLanguageText language={targetLanguage}>
                                {highlightTermInSentence(
                                  def.example_sentence_target_language,
                                  entry.display_text,
                                )}
                              </TargetLanguageText>
                            </div>
                            <div className="text-muted-foreground">
                              <span>{nativeLangCode}:</span>{" "}
                              {def.example_sentence_native_language}
                            </div>
                          </div>
                        </div>
                      ),
                    )
                  ) : (
                    <div className="pl-4 border-l-2 border-muted">
                      <div className="font-medium text-primary">
                        {entry.definition.Phrasebook.meaning}
                      </div>
                      {(entry.definition.Phrasebook.target_language_example ||
                        entry.definition.Phrasebook
                          .native_language_example) && (
                        <div className="mt-2 text-sm space-y-1">
                          {entry.definition.Phrasebook
                            .target_language_example && (
                            <div className="text-foreground">
                              <span className="text-muted-foreground">
                                {targetLangCode}:
                              </span>{" "}
                              <TargetLanguageText language={targetLanguage}>
                                {highlightTermInSentence(
                                  entry.definition.Phrasebook
                                    .target_language_example,
                                  entry.display_text,
                                )}
                              </TargetLanguageText>
                            </div>
                          )}
                          {entry.definition.Phrasebook
                            .native_language_example && (
                            <div className="text-muted-foreground">
                              <span>{nativeLangCode}:</span>{" "}
                              {
                                entry.definition.Phrasebook
                                  .native_language_example
                              }
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  )}
                </div>

                <div className="absolute bottom-3 right-3">
                  {isCardAdded(entry) ? (
                    <button
                      disabled
                      className="flex items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground cursor-default"
                    >
                      <CircleCheckBig className="w-4 h-4" />
                      <span>Added</span>
                    </button>
                  ) : (
                    <button
                      onClick={() => handleAddCard(entry)}
                      className="flex items-center gap-2 px-3 py-1.5 text-sm text-foreground hover:bg-muted rounded-md transition-colors"
                    >
                      <CirclePlus className="w-4 h-4" />
                      <span>Add to deck</span>
                    </button>
                  )}
                </div>
              </Card>
            );
          })}

          {entries.length === 0 && (
            <div className="text-center py-12 text-muted-foreground">
              {searchQuery
                ? "No entries found matching your search."
                : "No dictionary entries available."}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
