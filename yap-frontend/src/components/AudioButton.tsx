import { useState, useEffect, useCallback, useRef } from 'react'
import { Button } from "@/components/ui/button"
import { Volume2 } from "lucide-react"
import { playAudio } from "@/lib/utils"
import { type AudioRequest } from '../../../yap-frontend-rs/pkg'
import { isSoundEffectPlaying } from '@/lib/sound-effects'
import { toast } from 'sonner'

interface AudioButtonProps {
  audioRequest: AudioRequest
  accessToken: string | undefined
  autoPlay?: boolean
  className?: string
  size?: "default" | "sm" | "lg" | "icon"
  variant?: "default" | "destructive" | "outline" | "secondary" | "ghost" | "link"
}

export function AudioButton({
  audioRequest,
  accessToken,
  autoPlay = false,
  className = "h-10 w-10 shrink-0",
  size = "icon",
  variant = "ghost",
}: AudioButtonProps) {
  const [isPlaying, setIsPlaying] = useState(false)
  const [needsAuth, setNeedsAuth] = useState(false)
  const isPlayingRef = useRef(isPlaying)
  const clickedRef = useRef(false)


  // Keep ref in sync with state
  useEffect(() => {
    isPlayingRef.current = isPlaying
  }, [isPlaying])

  // Show toast when authentication is needed
  useEffect(() => {
    if (needsAuth && clickedRef.current) {
      console.log("Showing toast")
      toast.error('Please log in to play audio', {
        description: 'Audio playback requires an account to access the text-to-speech service.',
        duration: 5000,
      })
      setNeedsAuth(false) // Reset the state
    }
  }, [needsAuth])

  const handlePlayAudio = useCallback(async (e?: React.MouseEvent) => {
    e?.stopPropagation();
    if (isPlayingRef.current) return;

    setIsPlaying(true);
    try {
      // Wait for any currently playing sound effects to finish
      while (isSoundEffectPlaying()) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }

      await playAudio(audioRequest, accessToken, () => { if (clickedRef.current) { setNeedsAuth(true) } });
    } catch (error) {
      console.error('Failed to play audio:', error);
    } finally {
      setIsPlaying(false);
    }
  }, [JSON.stringify(audioRequest), accessToken]);

  // Auto-play audio when text changes (if autoPlay is enabled)
  useEffect(() => {
    if (!autoPlay) return;

    let cancelled = false;

    const playWithDelay = async () => {
      // Wait for any currently playing sound effects to finish
      while (isSoundEffectPlaying() && !cancelled) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }

      // Check if we should still play and we're not already playing
      if (!cancelled && !isPlayingRef.current) {
        handlePlayAudio();
      }
    };

    playWithDelay();

    // Cleanup function to prevent race conditions
    return () => {
      cancelled = true;
    };
  }, [audioRequest.request.text, audioRequest.request.language, audioRequest.provider, autoPlay, handlePlayAudio]); // Removed isPlaying and handlePlayAudio from deps

  return (
    <Button
      variant={variant}
      size={size}
      className={className}
      onClick={() => {
        console.log("Setting clickedRef.current to true")
        clickedRef.current = true
        handlePlayAudio()
      }}
      disabled={isPlaying}
      title="Play pronunciation"
    >
      <Volume2 className={`h-6 w-6 ${isPlaying ? 'animate-pulse' : ''} size--xl`} />
    </Button>
  );
}
