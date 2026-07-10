import { Link } from "react-router-dom";

export function About() {
  return (
    <div className="text-center text-xs text-muted-foreground mt-4">
      yap.town is created by{" "}
      <a href="https://twitter.com/chadnauseam" className="underline">
        André Popovitch
      </a>
      .{" "}
      <a href="https://github.com/yaptown/yap" className="underline">
        GitHub
      </a>
      {" | "}
      <a href="https://discord.gg/mpgqfsH" className="underline">
        Discord
      </a>
      {" | "}
      <Link to="/about" className="underline">
        About
      </Link>
      {" | "}
      <Link to="/privacy" className="underline">
        Privacy
      </Link>
      {" | "}
      <Link to="/terms" className="underline">
        Terms
      </Link>
    </div>
  );
}
