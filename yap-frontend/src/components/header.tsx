import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { ModeToggle } from "@/components/mode-toggle";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { NotificationSettings } from "@/components/notification-settings";
import { LogOut, AlertTriangle, ArrowLeft, User } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";
import { SyncStatusDialog } from "@/components/sync-status-dialog";
import type { UserInfo } from "@/App";
import { AuthDialog } from "@/components/auth-dialog";
import type { Language } from "../../../yap-frontend-rs/pkg";
import { match } from "ts-pattern";

interface HeaderProps {
  userInfo: UserInfo | undefined;
  onSignOut: () => void;
  onChangeLanguage?: () => void;
  showSignupNag?: boolean;
  language?: Language;
  backButton?: {
    label: string;
    onBack: () => void;
  };
  title?: string;
  dueCount?: number;
  dailyGoalPercent?: number;
}

function getLanguageEmoji(language: Language | undefined): string {
  return match(language)
    .with("French", () => "🇫🇷")
    .with("Spanish", () => "🇪🇸")
    .with("Korean", () => "🇰🇷")
    .with("English", () => "🇬🇧")
    .with("German", () => "🇩🇪")
    .with("Chinese", () => "🇨🇳")
    .with("Japanese", () => "🇯🇵")
    .with("Russian", () => "🇷🇺")
    .with("Portuguese", () => "🇵🇹")
    .with("Italian", () => "🇮🇹")
    .with(undefined, () => "🌍")
    .exhaustive();
}

export function Header({
  userInfo,
  onSignOut,
  onChangeLanguage,
  showSignupNag = false,
  language,
  backButton,
  title = "Yap.Town",
  dueCount,
  dailyGoalPercent,
}: HeaderProps) {
  const [authOpen, setAuthOpen] = useState(false);
  const [defaultView, setDefaultView] = useState<"signin" | "signup">("signin");
  const navigate = useNavigate();

  return (
    <div className="space-y-2">
      {dailyGoalPercent !== undefined && (
        <Progress
          value={Math.min(dailyGoalPercent, 100)}
          className="h-1 rounded-none fixed top-0 left-0 right-0 z-50 bg-transparent"
        />
      )}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-4">
          {backButton ? (
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="icon"
                onClick={backButton.onBack}
                className="h-8 w-10"
                title="Go back"
              >
                <ArrowLeft className="w-5 h-5" />
              </Button>
              <h1 className="text-2xl font-bold drop-shadow-[0_0px_8px_rgba(255,255,255,0.8)] dark:drop-shadow-[0_0px_8px_rgba(0,0,0,1)]">
                {backButton.label}
              </h1>
            </div>
          ) : (
            <>
              <div className="flex items-center gap-2">
                {onChangeLanguage ? (
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={onChangeLanguage}
                    className="h-8 w-10 text-2xl"
                    title="Change language"
                  >
                    {getLanguageEmoji(language)}
                  </Button>
                ) : (
                  <div className="h-8 w-10 flex items-center justify-center text-2xl">
                    {getLanguageEmoji(language)}
                  </div>
                )}
                <h1 className="text-2xl font-bold drop-shadow-[0_0px_8px_rgba(255,255,255,0.8)] dark:drop-shadow-[0_0px_8px_rgba(0,0,0,1)]">
                  <Link to="/about">
                    <span className="hidden sm:inline">{title}</span>
                  </Link>
                  <span className="sm:hidden">{title.split(".")[0]}</span>
                </h1>
              </div>
              {userInfo && (
                <div className="animate-fade-in-delayed">
                  <SyncStatusDialog />
                </div>
              )}
            </>
          )}
        </div>
        <div className="flex items-center gap-2">
          {dueCount !== undefined && dueCount > 0 && (
            <Badge
              variant="outline"
              className="text-xs text-muted-foreground border-muted-foreground"
            >
              {dueCount}
            </Badge>
          )}
          {userInfo ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  className="text-sm text-muted-foreground hover:text-foreground animate-fade-in-delayed gap-2"
                >
                  {userInfo.displayName || userInfo.email}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <NotificationSettings />
                <DropdownMenuItem
                  onClick={() => navigate(`/user/id/${userInfo.id}`)}
                >
                  <User className="mr-2 h-4 w-4" />
                  Profile
                </DropdownMenuItem>
                <DropdownMenuItem onClick={onSignOut}>
                  <LogOut className="mr-2 h-4 w-4" />
                  Sign Out
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : (
            <>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setDefaultView("signin");
                  setAuthOpen(true);
                }}
              >
                Sign In
              </Button>
              <AuthDialog
                open={authOpen}
                onOpenChange={setAuthOpen}
                defaultView={defaultView}
              />
            </>
          )}
          <ModeToggle />
        </div>
      </div>

      {!userInfo && showSignupNag && (
        <Card
          variant="light"
          className="p-3 flex-row items-center gap-3 mb-2 py-3"
        >
          <AlertTriangle className="h-5 w-5 text-muted-foreground flex-shrink-0" />
          <div className="flex-1">
            <p className="text-sm font-medium">
              Log in or create an account to make sure you don't lose your
              progress!
            </p>
            <p className="text-xs text-muted-foreground mt-0.5">
              Your learning data is currently only stored on this device.
            </p>
          </div>
          <Button
            onClick={() => {
              setDefaultView("signup");
              setAuthOpen(true);
            }}
            variant="outline"
            size="sm"
            className="flex-shrink-0"
          >
            Create Account
          </Button>
        </Card>
      )}
    </div>
  );
}
