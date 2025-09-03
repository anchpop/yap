import { cn } from "@/lib/utils"

interface CardsRemainingProps {
  dueCount: number
  totalCount: number
  className?: string
}

export function CardsRemaining({ dueCount, totalCount, className }: CardsRemainingProps) {
  return (
    <div className={cn("text-sm text-muted-foreground", className)}>
      {dueCount}/{totalCount} cards remaining
    </div>
  )
}

