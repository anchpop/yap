import { Button } from "@/components/ui/button";
import { MicOff } from "lucide-react";

interface CantSpeakButtonProps {
  onClick: () => void;
}

export function CantSpeakButton({ onClick }: CantSpeakButtonProps) {
  return (
    <Button
      onClick={onClick}
      variant="outline"
      className="w-full h-12 text-base font-medium backdrop-blur-sm"
    >
      <span className="relative flex items-center justify-center">
        <MicOff className="absolute right-full mr-2 h-5 w-5" />
        Can't speak now
      </span>
    </Button>
  );
}
