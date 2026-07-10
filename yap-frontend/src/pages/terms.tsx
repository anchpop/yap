import { Card } from "@/components/ui/card";
import { Link } from "react-router-dom";

const CONTACT_EMAIL = "support@yap.town";

export function TermsPage() {
  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex flex-col items-center justify-center p-4 gap-4">
        <div className="flex items-center mb-4 gap-2 w-full">
          <Link to="/" className="text-2xl">
            ←
          </Link>
          <h1 className="text-3xl font-bold">Terms of Service</h1>
        </div>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <div className="px-2 space-y-2">
            <p>
              Yap.Town is a free language-learning app built and operated by
              André Popovitch. By using it you agree to these terms — they're
              short, please actually read them.
            </p>
            <p className="text-sm">Effective July 9, 2026.</p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            The service
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Yap.Town teaches languages with spaced repetition and
              comprehensible input. It's under active development: features may
              change, break, or be removed, and we don't promise the service
              will always be available or that your data will never be lost —
              though we work hard to keep it safe and synced.
            </p>
            <p>
              AI-generated feedback (grading, pronunciation help, and the like)
              can be wrong. It's a study aid, not an authority.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Your account
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Keep your credentials to yourself; you're responsible for what
              happens under your account. You must be at least 13 years old to
              have one.
            </p>
            <p>
              Your display name and bio are visible to others — keep them
              civil. Don't impersonate people, don't harass anyone, don't try
              to break or overload the service, and don't use it for anything
              unlawful. We may suspend or remove accounts that do.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Your data and our content
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Your learning data is yours. We process it to run the service, as
              described in the{" "}
              <Link to="/privacy" className="underline text-foreground">
                privacy policy
              </Link>
              , and you can take it with you or delete it by contacting us.
            </p>
            <p>
              The app's source code is available on{" "}
              <a
                href="https://github.com/yaptown/yap"
                className="underline text-foreground"
              >
                GitHub
              </a>{" "}
              under its license. Example sentences and audio come from public
              corpora and other sources credited in the app.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Connected apps
          </h2>
          <div className="px-2 space-y-2">
            <p>
              If you connect an AI assistant or other app to your account (see{" "}
              <Link to="/mcp" className="underline text-foreground">
                the connector
              </Link>
              ), it acts on your behalf: reviews it logs and cards it adds are
              treated as yours. Only connect apps you trust, and we may revoke
              access for connected apps that abuse the service.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            The legal bits
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Yap.Town is provided "as is", free of charge, without warranties
              of any kind. To the maximum extent permitted by law, we are not
              liable for damages arising from your use of the service. Nothing
              in these terms limits liability that can't legally be limited.
            </p>
            <p>
              We may update these terms; if we make significant changes we'll
              say so in the app, and continuing to use yap after that means you
              accept the new terms. Questions:{" "}
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
