import { cn } from "@/lib/utils";

function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="skeleton"
      className={cn(
        "bg-green-500/10 border-green-500/20 animate-pulse rounded-md",
        className,
      )}
      {...props}
    />
  );
}

export { Skeleton };
