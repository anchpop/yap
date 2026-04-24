import { Moon, Sun, Monitor, Check, Zap } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTheme } from "@/components/theme-provider";
import { cn } from "@/lib/utils";

export function ModeToggle() {
  const {
    theme,
    setTheme,
    animatedBackground,
    toggleAnimatedBackground,
    mouseFollow,
    toggleMouseFollow,
  } = useTheme();
  const isDarkTheme = theme === "dark" || theme === "oled";

  const themes = [
    { value: "light" as const, icon: Sun, label: "Light" },
    { value: "dark" as const, icon: Moon, label: "Dark" },
    { value: "oled" as const, icon: Zap, label: "OLED" },
    { value: "system" as const, icon: Monitor, label: "System" },
  ];

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="text-muted-foreground">
          <Sun className="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0" />
          <Moon className="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
          <span className="sr-only">Toggle theme</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        <div className="grid grid-cols-4 gap-1 p-1">
          {themes.map(({ value, icon: Icon, label }) => (
            <button
              key={value}
              onClick={() => setTheme(value)}
              className={cn(
                "flex flex-col items-center gap-1 rounded-md p-2 hover:bg-accent transition-colors",
                theme === value && "bg-accent",
              )}
            >
              <Icon
                className={cn(
                  "h-4 w-4",
                  theme === value ? "text-foreground" : "text-muted-foreground",
                )}
              />
              <span
                className={cn(
                  "text-xs",
                  theme === value
                    ? "text-foreground font-medium"
                    : "text-muted-foreground",
                )}
              >
                {label}
              </span>
            </button>
          ))}
        </div>
        <DropdownMenuSeparator />
        <button
          onClick={toggleAnimatedBackground}
          className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent transition-colors"
        >
          <Check
            className={cn(
              "h-4 w-4",
              animatedBackground ? "opacity-100" : "opacity-0",
            )}
          />
          <span>Animated background</span>
        </button>
        {isDarkTheme && animatedBackground && (
          <button
            onClick={toggleMouseFollow}
            className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent transition-colors"
          >
            <Check
              className={cn(
                "h-4 w-4",
                mouseFollow ? "opacity-100" : "opacity-0",
              )}
            />
            <span>Mouse follow</span>
          </button>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
