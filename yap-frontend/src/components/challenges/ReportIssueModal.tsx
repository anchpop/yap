import { useState } from "react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { supabase } from "@/lib/supabase"
import type { Language } from "../../../../yap-frontend-rs/pkg/yap_frontend_rs"

interface ReportIssueModalProps {
  context: string
  open: boolean
  onOpenChange: (open: boolean) => void
  targetLanguage: Language
}

export function ReportIssueModal({ context, open, onOpenChange, targetLanguage }: ReportIssueModalProps) {
  const [issueText, setIssueText] = useState("")
  const [isSubmitting, setIsSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    if (!issueText.trim()) {
      return
    }

    setIsSubmitting(true)

    try {
      const { data: { user } } = await supabase.auth.getUser()

      if (!user) {
        throw new Error("Must be logged in to report issues")
      }

      const { error } = await supabase
        .from('issues')
        .insert({
          user_id: user.id,
          issue_text: `Language: ${targetLanguage}\n\nContext: ${context}\n\nIssue: ${issueText.trim()}`
        })

      if (error) {
        console.error('Error submitting issue:', error)
        throw error
      }

      setIssueText("")
      onOpenChange(false)
    } catch (error) {
      console.error('Failed to submit issue:', error)
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Report an Issue</DialogTitle>
          <DialogDescription>
            Describe the issue you're experiencing. We'll look into it as soon as possible.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit}>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="issue-text">Issue description</Label>
              <Textarea
                id="issue-text"
                placeholder="Please describe the issue you're experiencing..."
                value={issueText}
                onChange={(e) => setIssueText(e.target.value)}
                className="min-h-[100px]"
                required
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting || !issueText.trim()}>
              {isSubmitting ? "Submitting..." : "Submit Issue"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
