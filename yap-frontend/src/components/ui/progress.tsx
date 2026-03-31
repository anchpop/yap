import * as React from "react"
import * as ProgressPrimitive from "@radix-ui/react-progress"

import { cn } from "@/lib/utils"

function Progress({
  className,
  value,
  projectedValue,
  disableTransition,
  showPercentage,
  label,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root> & {
  projectedValue?: number
  disableTransition?: boolean
  showPercentage?: boolean
  label?: string
}) {
  const pct = value || 0
  const projectedPct = projectedValue ?? pct
  const displayLabel = label ?? `${Math.floor(pct)}%`

  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      className={cn(
        "bg-primary/10 relative h-2 w-full overflow-hidden rounded-full",
        className
      )}
      {...props}
    >
      {projectedPct > pct && (
        <div
          className={cn("bg-primary/30 absolute h-full", !disableTransition && "transition-all")}
          style={{ width: `${projectedPct}%` }}
        />
      )}
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className={cn("bg-primary h-full w-full flex-1", !disableTransition && "transition-all")}
        style={{ transform: `translateX(-${100 - pct}%)` }}
      />
      {(showPercentage || label) && (
        <>
          {/* Text over unfilled portion */}
          <span
            className={cn(
              "absolute inset-0 flex items-center justify-center text-sm font-mono font-semibold text-primary",
              !disableTransition && "transition-all"
            )}
            style={{ clipPath: `inset(0 0 0 ${pct}%)` }}
          >
            {displayLabel}
          </span>
          {/* Text over filled portion */}
          <span
            className={cn(
              "absolute inset-0 flex items-center justify-center text-sm font-mono font-semibold text-primary-foreground",
              !disableTransition && "transition-all"
            )}
            style={{ clipPath: `inset(0 ${100 - pct}% 0 0)` }}
          >
            {displayLabel}
          </span>
        </>
      )}
    </ProgressPrimitive.Root>
  )
}

export { Progress }
