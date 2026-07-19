import { Card } from "@/components/ui/card";
import { Link } from "react-router-dom";

const CONTACT_EMAIL = "support@yap.town";

export function PrivacyPage() {
  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex flex-col items-center justify-center p-4 gap-4">
        <div className="flex items-center mb-4 gap-2 w-full">
          <Link to="/" className="text-2xl">
            ←
          </Link>
          <h1 className="text-3xl font-bold">Privacy Policy</h1>
        </div>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <div className="px-2 space-y-2">
            <p>
              Yap.Town is a language-learning app built and operated by André
              Popovitch. The short version: we collect what's needed to teach
              you a language and sync your progress between devices, we don't
              sell your data, we don't run ads, and there are no advertising or
              analytics trackers on the site.
            </p>
            <p className="text-sm">Effective July 19, 2026.</p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            What we collect
          </h2>
          <div className="px-2 space-y-2">
            <p>
              <span className="text-foreground">Account.</span> Your email
              address and password (stored hashed by our auth provider). If
              you add a passkey, our auth provider stores its public key; we
              never receive the private key or your biometric data. You can
              optionally add a display name and bio, which are visible to
              other users.
            </p>
            <p>
              <span className="text-foreground">Learning activity.</span> The
              words you add, your reviews and ratings, streaks, XP, and course
              selection. This is the heart of the app: it's stored on our
              servers so your progress syncs between devices, and a copy lives
              on your device so yap works offline.
            </p>
            <p>
              <span className="text-foreground">Social.</span> Who you follow
              and who follows you.
            </p>
            <p>
              <span className="text-foreground">Practice audio and answers.</span>{" "}
              When you use pronunciation practice, your microphone recording is
              sent to our server and to AI providers to generate feedback. When
              you type answers to challenges, the answer is sent to AI
              providers for grading. These are processed to give you feedback,
              not to build a profile of you.
            </p>
            <p>
              <span className="text-foreground">Technical.</span> We use Sentry
              for error and performance monitoring so we can fix crashes. Error
              reports can include your IP address and device information, and a
              sample of sessions is recorded as a session replay (a
              reconstruction of what the app showed and where you clicked) to
              help us debug.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Services we rely on
          </h2>
          <div className="px-2 space-y-2">
            <p>
              We don't run our own data centers. These providers process data
              on our behalf, each only for the purpose listed:
            </p>
            <ul className="list-disc pl-5 space-y-1">
              <li>Supabase — accounts, authentication, and the database</li>
              <li>Cloudflare — hosting and content delivery</li>
              <li>Fly.io — backend servers</li>
              <li>Sentry — error tracking and session replay</li>
              <li>Resend — sending emails, such as notifications</li>
              <li>Amazon Web Services — some backend processing and storage</li>
              <li>
                AI providers (Google, OpenAI, ElevenLabs) — grading practice
                answers, pronunciation feedback, and generating audio
              </li>
            </ul>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            AI assistants you connect
          </h2>
          <div className="px-2 space-y-2">
            <p>
              You can connect AI assistants like Claude or ChatGPT to your yap
              account through our{" "}
              <Link to="/mcp" className="underline text-foreground">
                connector
              </Link>
              . If you do, that assistant can see your deck, stats, and
              dictionary, and can add cards and log reviews on your behalf —
              only after you approve the connection while signed in. Your
              conversations with the assistant happen in that provider's
              product and are governed by its privacy policy, not this one; we
              only receive the requests the assistant makes to your account.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Retention and deletion
          </h2>
          <div className="px-2 space-y-2">
            <p>
              We keep your data for as long as your account exists. To delete
              your account and its synced data, email{" "}
              <a
                href={`mailto:${CONTACT_EMAIL}`}
                className="underline text-foreground"
              >
                {CONTACT_EMAIL}
              </a>{" "}
              from your account's email address — we'll remove it from our live
              systems promptly, and from backups as they rotate out. The
              offline copy on your device is under your control; clearing your
              browser's site data removes it.
            </p>
            <p>
              You can also email us to access, export, or correct your data.
              If you're covered by GDPR, CCPA, or similar laws, these are the
              rights they give you, and we honor them for everyone regardless
              of where you live.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Children
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Yap.Town is not directed at children under 13, and you must be at
              least 13 to create an account. If you believe a child under 13
              has an account, contact us and we'll delete it.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Changes and contact
          </h2>
          <div className="px-2 space-y-2">
            <p>
              If this policy changes, we'll update this page and its effective
              date; if the changes are significant we'll say so in the app.
              Questions, requests, or concerns:{" "}
              <a
                href={`mailto:${CONTACT_EMAIL}`}
                className="underline text-foreground"
              >
                {CONTACT_EMAIL}
              </a>
              .
            </p>
          </div>
        </Card>
      </div>
    </div>
  );
}
