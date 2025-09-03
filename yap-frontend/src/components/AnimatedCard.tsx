import { motion } from "framer-motion"
import { cn } from "@/lib/utils"

interface AnimatedCardProps extends React.ComponentProps<typeof motion.div> {}

export function AnimatedCard({ className, ...props }: AnimatedCardProps) {
  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.3 }}
      className={cn(className)}
      {...props}
    />
  )
}

