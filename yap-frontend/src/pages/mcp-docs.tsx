import { Card } from "@/components/ui/card";
import { Link } from "react-router-dom";

const CONTACT_EMAIL = "support@yap.town";
const SECURITY_EMAIL = "security@yap.town";
const CONNECTOR_URL = "https://mcp.yap.town/mcp";

export function McpDocsPage() {
  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex flex-col items-center justify-center p-4 gap-4">
        <div className="flex items-center mb-4 gap-2 w-full">
          <Link to="/" className="text-2xl">
            ←
          </Link>
          <h1 className="text-3xl font-bold">Yap for AI Assistants</h1>
        </div>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <div className="px-2 space-y-2">
            <p>
              Yap has a connector (an{" "}
              <a
                href="https://modelcontextprotocol.io"
                className="underline text-foreground"
              >
                MCP server
              </a>
              ) that lets AI assistants like Claude and ChatGPT use your
              yap.town account. Do your reviews in the middle of a
              conversation, ask what a word means and add it to your deck, or
              have the assistant quiz you with sentences you can actually
              understand — everything syncs with the app.
            </p>
            <p>
              The connector URL is{" "}
              <code className="text-foreground">{CONNECTOR_URL}</code>.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            What it can do
          </h2>
          <div className="px-2 space-y-2">
            <ul className="list-disc pl-5 space-y-1">
              <li>List the flashcards you're due to review</li>
              <li>
                Quiz you with example sentences made of words you already know,
                and log your reviews — real spaced-repetition scheduling, same
                as in the app
              </li>
              <li>Search the dictionary of your course</li>
              <li>Add new words and phrases to your deck</li>
              <li>Release cards you've set aside in lockup</li>
              <li>Show your stats: streak, XP, deck size, comprehension tier</li>
            </ul>
            <p>
              That's the full list. It can't change your course, delete cards,
              edit your profile, or touch your account settings.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Connecting
          </h2>
          <div className="px-2 space-y-2">
            <p>
              <span className="text-foreground">Claude:</span> Settings →
              Connectors → Add custom connector, and paste the connector URL.
            </p>
            <p>
              <span className="text-foreground">ChatGPT:</span> Settings → Apps
              &amp; Connectors, enable Developer mode under Advanced settings,
              then add the connector URL (requires a paid ChatGPT plan).
            </p>
            <p>
              Either way, you'll be sent to yap.town to sign in and approve the
              connection. Approve it only if you initiated it. To disconnect,
              remove the connector in the assistant's settings.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Privacy
          </h2>
          <div className="px-2 space-y-2">
            <p>
              A connected assistant can read your deck, stats, and dictionary,
              and can add cards and log reviews. It never receives your yap
              password or a general-purpose credential — the connection uses
              scoped tokens that only work with the connector. Your
              conversations with the assistant are handled by its provider
              under its own privacy policy; see also{" "}
              <Link to="/privacy" className="underline text-foreground">
                yap's privacy policy
              </Link>
              .
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Troubleshooting
          </h2>
          <div className="px-2 space-y-2">
            <p>
              <span className="text-foreground">
                "Could not detect course" or an empty deck:
              </span>{" "}
              the connector follows the course you use in the app. Open{" "}
              <a href="https://yap.town" className="underline text-foreground">
                yap.town
              </a>
              , pick a course, and add a few words first.
            </p>
            <p>
              <span className="text-foreground">Connection fails:</span> make
              sure you completed the sign-in and pressed Connect on the
              yap.town approval page, signed in to the account you meant to
              use. Then remove the connector in the assistant and add it again.
            </p>
            <p>
              <span className="text-foreground">Stale data:</span> the
              connector re-syncs with your account within about 20 seconds. If
              a review you just did in the app isn't reflected, ask the
              assistant to check again.
            </p>
            <p>
              <span className="text-foreground">"Too many requests":</span> the
              server rate-limits sign-in attempts per address; wait a minute
              and retry.
            </p>
          </div>
        </Card>

        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Support and security
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Problems or questions:{" "}
              <a
                href={`mailto:${CONTACT_EMAIL}`}
                className="underline text-foreground"
              >
                {CONTACT_EMAIL}
              </a>
              .
            </p>
            <p>
              Found a security vulnerability in the connector or in yap? Please
              report it privately to{" "}
              <a
                href={`mailto:${SECURITY_EMAIL}`}
                className="underline text-foreground"
              >
                {SECURITY_EMAIL}
              </a>{" "}
              and give us a chance to fix it before disclosing. We read and
              investigate every report.
            </p>
          </div>
        </Card>
      </div>
    </div>
  );
}
