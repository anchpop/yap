import { useState } from 'react'
import { Button } from "@/components/ui/button"
import { ModeToggle } from "@/components/mode-toggle"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { NotificationSettings } from '@/components/notification-settings'
import { LogOut, AlertTriangle, Languages } from 'lucide-react'
import { SyncStatusDialog } from '@/components/sync-status-dialog'
import type { UserInfo } from '@/App'
import { AuthDialog } from '@/components/auth-dialog'

interface HeaderProps {
  userInfo: UserInfo | undefined
  onSignOut: () => void
  onChangeLanguage?: () => void
  showSignupNag?: boolean
}

export function Header({
  userInfo,
  onSignOut,
  onChangeLanguage,
  showSignupNag = false,
}: HeaderProps) {
  const [authOpen, setAuthOpen] = useState(false)
  const [defaultView, setDefaultView] = useState<'signin' | 'signup'>('signin')

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <img src="/yap.svg" alt="Yap.Town logo" className="h-8 w-8 rounded-md" />
            <h1 className="text-2xl font-bold">
              <span className="hidden sm:inline">Yap.Town</span>
              <span className="sm:hidden">Yap</span>
            </h1>
          </div>
          {userInfo && (
            <SyncStatusDialog />
          )}
        </div>
        <div className="flex items-center gap-2">
          {userInfo ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" className="text-sm text-muted-foreground hover:text-foreground">
                  {userInfo.email}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <NotificationSettings />
                {onChangeLanguage && (
                  <DropdownMenuItem onClick={onChangeLanguage}>
                    <Languages className="mr-2 h-4 w-4" />
                    Language
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem onClick={onSignOut}>
                  <LogOut className="mr-2 h-4 w-4" />
                  Sign Out
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : (
            <>
              <Button
                variant="default"
                size="sm"
                onClick={() => {
                  setDefaultView('signin')
                  setAuthOpen(true)
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
        <div className="bg-muted/50 border rounded-lg p-3 flex items-center gap-3 mb-2">
          <AlertTriangle className="h-5 w-5 text-muted-foreground flex-shrink-0" />
          <div className="flex-1">
            <p className="text-sm font-medium">
              Log in or create an account to make sure you don't lose your progress!
            </p>
            <p className="text-xs text-muted-foreground mt-0.5">
              Your learning data is currently only stored on this device.
            </p>
          </div>
          <Button
            onClick={() => {
              setDefaultView('signup')
              setAuthOpen(true)
            }}
            variant="outline"
            size="sm"
            className="flex-shrink-0"
          >
            Create Account
          </Button>
        </div>
      )}
    </div>
  )
}
