import { useState, useEffect } from 'react'
import { useSearchParams, useNavigate } from 'react-router-dom'
import { supabase } from '@/lib/supabase'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { ThemeProvider } from '@/components/theme-provider'

export function ResetPassword() {
  const [searchParams] = useSearchParams()
  const navigate = useNavigate()
  const [loading, setLoading] = useState(false)
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)
  const [isValidToken, setIsValidToken] = useState<boolean | null>(null)

  useEffect(() => {
    // Check both query params and hash fragments
    let access_token = searchParams.get('access_token')
    let refresh_token = searchParams.get('refresh_token')

    // If not in query params, check hash fragments
    if (!access_token && window.location.hash) {
      const hashParams = new URLSearchParams(window.location.hash.substring(1))
      access_token = hashParams.get('access_token')
      refresh_token = hashParams.get('refresh_token')
    }

    if (access_token && refresh_token) {
      // Standard Supabase password reset flow - just set the session
      supabase.auth.setSession({
        access_token,
        refresh_token,
      }).then(({ data, error }) => {
        if (error) {
          console.error('Error setting session:', error)
          setError('Invalid or expired reset link. Please request a new one.')
          setIsValidToken(false)
        } else {
          console.log('Session set successfully:', data)
          setIsValidToken(true)
          // Clear the URL to hide the tokens
          window.history.replaceState(null, '', window.location.pathname)
        }
      })
    } else {
      console.log('No tokens found in URL')
      setError('Invalid reset link. Please request a new one.')
      setIsValidToken(false)
    }
  }, [searchParams])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    
    if (password !== confirmPassword) {
      setError('Passwords do not match')
      return
    }

    if (password.length < 6) {
      setError('Password must be at least 6 characters')
      return
    }

    setLoading(true)

    const { error } = await supabase.auth.updateUser({
      password: password
    })

    if (error) {
      setError(error.message)
    } else {
      setSuccess(true)
      setTimeout(() => {
        navigate('/')
      }, 2000)
    }
    setLoading(false)
  }

  if (success) {
    return (
      <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl font-bold text-green-500">Password Updated!</CardTitle>
              <CardDescription>Your password has been successfully updated. Redirecting...</CardDescription>
            </CardHeader>
          </Card>
        </div>
      </ThemeProvider>
    )
  }

  // Show loading while verifying token
  if (isValidToken === null) {
    return (
      <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl font-bold">Verifying Reset Link</CardTitle>
              <CardDescription>Please wait...</CardDescription>
            </CardHeader>
          </Card>
        </div>
      </ThemeProvider>
    )
  }

  // Show error if token is invalid
  if (isValidToken === false) {
    return (
      <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl font-bold">Invalid Reset Link</CardTitle>
              <CardDescription>This password reset link is invalid or has expired.</CardDescription>
            </CardHeader>
            <CardContent className="text-center">
              {error && (
                <div className="mb-4 p-3 rounded text-sm bg-destructive/10 text-destructive border border-destructive/20">
                  {error}
                </div>
              )}
              <Button onClick={() => navigate('/forgot-password')} variant="outline">
                Request New Reset Link
              </Button>
            </CardContent>
          </Card>
        </div>
      </ThemeProvider>
    )
  }

  return (
    <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
      <div className="min-h-screen bg-background flex items-center justify-center p-4">
        <Card className="w-full max-w-md">
          <CardHeader className="text-center">
            <CardTitle className="text-2xl font-bold">Reset Your Password</CardTitle>
            <CardDescription>Enter your new password below</CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="password">New Password</Label>
                <Input
                  id="password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  minLength={6}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="confirm-password">Confirm New Password</Label>
                <Input
                  id="confirm-password"
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  required
                  minLength={6}
                />
              </div>
              <Button type="submit" className="w-full" disabled={loading}>
                {loading ? 'Updating...' : 'Update Password'}
              </Button>
            </form>

            {error && (
              <div className="mt-4 p-3 rounded text-sm bg-destructive/10 text-destructive border border-destructive/20">
                {error}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </ThemeProvider>
  )
}
