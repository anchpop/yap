import { useState, useMemo } from "react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { update_profile } from "../../../yap-frontend-rs/pkg";
import { toast } from "sonner";
import { Sparkles } from "lucide-react";

// Language learning themed name components
const ADJECTIVES = [
  "Fluent",
  "Polyglot",
  "Eloquent",
  "Bilingual",
  "Linguistic",
  "Verbal",
  "Articulate",
  "Multilingual",
  "Speaking",
  "Learning",
  "Studying",
  "Practicing",
  "Reading",
  "Writing",
  "Listening",
  "Translating",
  "Conversing",
  "Chatting",
  "Global",
  "Worldly",
  "Cultural",
  "International",
  "Traveling",
  "Curious",
  "Dedicated",
  "Persistent",
  "Motivated",
  "Eager",
  "Passionate",
  "Enthusiastic",
  "Ambitious",
  "Diligent",
  "Focused",
  "Committed",
  "Determined",
  "Aspiring",
  "Emerging",
  "Rising",
  "Growing",
  "Developing",
  "Advancing",
  "Progressing",
  "Native",
  "Expert",
  "Master",
  "Scholar",
  "Student",
  "Learner",
];

const NOUNS = [
  "Linguist",
  "Polyglot",
  "Scholar",
  "Student",
  "Learner",
  "Speaker",
  "Reader",
  "Writer",
  "Translator",
  "Interpreter",
  "Communicator",
  "Conversationalist",
  "Explorer",
  "Traveler",
  "Wanderer",
  "Adventurer",
  "Nomad",
  "Globetrotter",
  "Enthusiast",
  "Devotee",
  "Aficionado",
  "Fan",
  "Buff",
  "Lover",
  "Prodigy",
  "Genius",
  "Talent",
  "Wizard",
  "Expert",
  "Master",
  "Apprentice",
  "Novice",
  "Beginner",
  "Starter",
  "Rookie",
  "Freshman",
  "Teacher",
  "Tutor",
  "Coach",
  "Mentor",
  "Guide",
  "Instructor",
  "Champion",
  "Star",
  "Ace",
  "Pro",
  "Virtuoso",
  "Maestro",
];

function generateRandomName(): string {
  const adjective = ADJECTIVES[Math.floor(Math.random() * ADJECTIVES.length)];
  const noun = NOUNS[Math.floor(Math.random() * NOUNS.length)];
  const number = Math.floor(Math.random() * 1000);
  return `${adjective}${noun}${number}`;
}

interface SetDisplayNameProps {
  accessToken: string;
  onComplete: () => void;
  onSkip: () => void;
  totalReviewsCompleted: bigint;
}

export function SetDisplayName({
  accessToken,
  onComplete,
  onSkip,
  totalReviewsCompleted,
}: SetDisplayNameProps) {
  const randomName = useMemo(() => generateRandomName(), []);
  const [displayName, setDisplayName] = useState(randomName);
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    if (!displayName.trim()) {
      toast.error("Please enter a display name");
      return;
    }

    try {
      setSaving(true);
      await update_profile(displayName.trim(), null, accessToken);
      toast.success("Display name set!");
      onComplete();
    } catch (err) {
      console.error("Error setting display name:", err);
      toast.error("Failed to set display name. Please try again.");
    } finally {
      setSaving(false);
    }
  };

  const handleSkip = () => {
    onSkip();
  };

  return (
    <div className="flex items-center justify-center w-full">
      <Card className="w-full">
        <CardHeader>
          <div className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            <CardTitle>Choose Your Display Name</CardTitle>
          </div>
          <CardDescription>
            You've completed {totalReviewsCompleted} reviews! Set a display name
            to personalize your profile.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="displayName">Display Name</Label>
            <Input
              id="displayName"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Enter your display name"
              maxLength={50}
              disabled={saving}
            />
            <p className="text-xs text-muted-foreground">
              You can change this at any time.
            </p>
          </div>

          <div className="flex gap-2">
            <Button
              onClick={handleSkip}
              variant="outline"
              className="flex-1"
              disabled={saving}
            >
              Skip
            </Button>
            <Button onClick={handleSave} className="flex-1" disabled={saving}>
              {saving ? "Saving..." : "Save"}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
