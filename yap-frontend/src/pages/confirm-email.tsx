import { useState, useEffect } from 'react'
import { useSearchParams, useNavigate } from 'react-router-dom'
import { supabase } from '@/lib/supabase'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { ThemeProvider } from '@/components/theme-provider'

export function ConfirmEmail() {
  const [searchParams] = useSearchParams()
  const navigate = useNavigate()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  const token_hash = searchParams.get('token_hash')
  const type = searchParams.get('type')

  useEffect(() => {
    const confirmEmail = async () => {
      if (token_hash && type) {
        const { error } = await supabase.auth.verifyOtp({
          token_hash,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          type: type as any,
        })

        if (error) {
          setError(error.message)
        } else {
          setSuccess(true)
          setTimeout(() => {
            navigate('/')
          }, 3000)
        }
      } else {
        setError('Invalid confirmation link')
      }
      setLoading(false)
    }

    confirmEmail()
  }, [token_hash, type, navigate])

  const handleReturnHome = () => {
    navigate('/')
  }

  if (loading) {
    return (
      <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl font-bold">Confirming Email...</CardTitle>
              <CardDescription>Please wait while we confirm your email address</CardDescription>
            </CardHeader>
            <CardContent className="text-center">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto"></div>
            </CardContent>
          </Card>
        </div>
      </ThemeProvider>
    )
  }

  if (success) {
    return (
      <ThemeProvider defaultTheme="dark" storageKey="vite-ui-theme">
        <div className="min-h-screen bg-background flex items-center justify-center p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl font-bold text-green-500">Email Confirmed!</CardTitle>
              <CardDescription>Your email has been successfully confirmed. You can now sign in to your account.</CardDescription>
            </CardHeader>
            <CardContent className="text-center">
              <p className="text-sm text-muted-foreground mb-4">Redirecting to home page...</p>
              <Button onClick={handleReturnHome} variant="outline">
                Go to Home
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
            <CardTitle className="text-2xl font-bold text-destructive">Confirmation Failed</CardTitle>
            <CardDescription>There was an issue confirming your email address</CardDescription>
          </CardHeader>
          <CardContent className="text-center">
            {error && (
              <div className="mb-4 p-3 rounded text-sm bg-destructive/10 text-destructive border border-destructive/20">
                {error}
              </div>
            )}
            <Button onClick={handleReturnHome} variant="outline">
              Return to Home
            </Button>
          </CardContent>
        </Card>
      </div>
    </ThemeProvider>
  )
}
