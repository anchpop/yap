import { useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import TimeAgo from 'react-timeago'
import type { Deck } from '../../../yap-frontend-rs/pkg'

interface StatsProps {
  deck: Deck
}

export function Stats({ deck }: StatsProps) {
  const reviewInfo = deck.get_review_info([])
  const allCardsSummary = deck.get_all_cards_summary();

  const now = Date.now();
  const dueCards = allCardsSummary.filter((card) => card.due_timestamp_ms <= now);
  const notDueCards = allCardsSummary.filter((card) => card.due_timestamp_ms > now);

  const [visibleCount, setVisibleCount] = useState(10)
  const visibleCards = [...dueCards, ...notDueCards.slice(0, visibleCount)]

  return (
    <div className="mt-4">
      <div className="mb-4">
        <h2 className="text-2xl font-semibold">Stats</h2>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-3">
          <div className="bg-card border rounded-lg p-4">
            <p className="text-sm text-muted-foreground mb-1">Total Cards</p>
            <p className="text-2xl font-bold">{allCardsSummary.length}</p>
            <p className="text-sm text-muted-foreground mt-1">{reviewInfo.due_count || 0} due now</p>
          </div>
          <div className="bg-card border rounded-lg p-4">
            <p className="text-sm text-muted-foreground mb-1">Words Known</p>
            <p className="text-2xl font-bold">{(deck.get_percent_of_words_known() * 100).toFixed(2)}%</p>
            <p className="text-sm text-muted-foreground mt-1">of total</p>
          </div>
          <div className="bg-card border rounded-lg p-4">
            <p className="text-sm text-muted-foreground mb-1">Daily Streak</p>
            <p className="text-2xl font-bold">{deck.get_daily_streak()}</p>
            <p className="text-sm text-muted-foreground mt-1">days</p>
          </div>
          <div className="bg-card border rounded-lg p-4">
            <p className="text-sm text-muted-foreground mb-1">Total Reviews</p>
            <p className="text-2xl font-bold">{deck.get_total_reviews()}</p>
            <p className="text-sm text-muted-foreground mt-1">all time</p>
          </div>
        </div>
      </div>
      <div className="bg-card border rounded-lg overflow-hidden">
        <table className="w-full table-fixed">
          <thead>
            <tr className="border-b bg-muted/50">
              <th className="text-left p-3 font-medium w-1/4">Word</th>
              <th className="text-left p-3 font-medium w-1/4">State</th>
              <th className="text-left p-3 font-medium w-1/2">Due</th>
            </tr>
          </thead>
          <tbody>
            {visibleCards.map((card, index) => {
              const shortDescription =
                "TargetLanguage" in card.card_indicator ?
                  "Heteronym" in card.card_indicator.TargetLanguage.lexeme ?
                    card.card_indicator.TargetLanguage.lexeme.Heteronym.word :
                    card.card_indicator.TargetLanguage.lexeme.Multiword :
                  `/${card.card_indicator.ListeningHomophonous.pronunciation}/`

              const isDue = card.due_timestamp_ms <= now;
              return (
                <tr key={index} className={`border-b ${isDue ? 'bg-green-500/10' : ''}`}>
                  <td className="p-3 font-medium">{shortDescription}</td>
                  <td className="p-3">
                    <Badge variant="outline">{card.state}</Badge>
                  </td>
                  <td className="p-3 text-sm text-muted-foreground">
                    {isDue ? (
                      <span className="text-green-500 font-medium">Due now</span>
                    ) : (
                      <TimeAgo date={new Date(card.due_timestamp_ms)} />
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {notDueCards.length > visibleCount && (
          <div className="p-3">
            <Button variant="outline" onClick={() => setVisibleCount((c) => c + 10)}>
              Show more
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
