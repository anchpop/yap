import { Card } from "@/components/ui/card";
import { Link } from "react-router-dom";
import { InstallPwaButton } from "@/components/InstallPwaButton";

export function AboutPage() {
  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex flex-col items-center justify-center p-4 gap-4">
        <div className="flex items-center mb-4 gap-2 w-full">
          <Link to="/" className="text-2xl">
            ←
          </Link>
          <h1 className="text-3xl font-bold">About Yap.Town</h1>
        </div>
        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Philosophy
          </h2>
          <div className="px-2 space-y-2">
            <p>
              I believe that efficient and effective language learning options
              should be available and accessible to everyone.
            </p>
            <p>
              The most famous concepts in language learning are spaced
              repetition and comprehensible input. I wanted to combine them into
              one app, and make it free for all to use.
            </p>
          </div>
        </Card>
        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            How It Works
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Yap tries to keep track of what words you know show it can show
              you sentences that contain only those words. This is called
              "comprehensible input".
            </p>
            <p>
              Furthermore, Yap selects sentences that also contain words that
              you need to review. By spacing out these reviews over time, you
              can learn an immense amount of vocabulary and grammar in a short
              amount of time. This is called "spaced repetition".
            </p>
            <p>
              This approach helps you build vocabulary and grammar intuition
              through context, the same way you learned your first language.
            </p>
          </div>
        </Card>
        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Offline First PWA
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Yap.Town works fully offline! This means you can learn anywhere
              without an internet connection.
            </p>
            <p>
              To make use of this, you should install Yap.Town as a Progressive
              Web App (PWA).
            </p>
            <div className="pt-2">
              <InstallPwaButton />
            </div>
          </div>
        </Card>
        <Card className="max-w-2xl w-full p-8 gap-2 text-muted-foreground">
          <h2 className="text-xl font-semibold text-foreground mb-2">
            Who am I?
          </h2>
          <div className="px-2 space-y-2">
            <p>
              Created by{" "}
              <a
                href="https://twitter.com/chadnauseam"
                className="underline text-foreground"
              >
                André Popovitch
              </a>
              .{" "}
            </p>
            <p>
              <a
                href="https://github.com/yaptown/yap"
                className="underline text-foreground"
              >
                View on GitHub
              </a>
              {" | "}
              <a
                href="https://discord.gg/mpgqfsH"
                className="underline text-foreground"
              >
                Join the Discord
              </a>
            </p>
            <p>Here is a picture of me and my girlfriend Emma :P</p>
            <img
              src="/selfie.webp"
              alt="A man and woman dressed formally. Both are smiling and have red hair."
              className="w-full rounded-lg"
            />
          </div>
        </Card>
      </div>
    </div>
  );
}
