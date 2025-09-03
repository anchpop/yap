import { Button } from "@/components/ui/button"
import { VolumeX } from "lucide-react"

interface CantListenButtonProps {
  onClick: () => void
}

export function CantListenButton({ onClick }: CantListenButtonProps) {
  return (
    <Button
      onClick={onClick}
      variant="outline"
      className="w-full h-12 text-base font-medium"
    >
      <VolumeX className="mr-2 h-5 w-5" />
      Can't listen now
    </Button>
  )
}
