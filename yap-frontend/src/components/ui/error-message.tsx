import { useState } from "react";
import { Copy, Check, ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface ErrorMessageProps {
  message: string;
  title?: string;
  variant?: "default" | "compact";
  className?: string;
}

export function ErrorMessage({
  message,
  title,
  variant = "default",
  className,
}: ErrorMessageProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard
      .writeText(message)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      })
      .catch((err) => {
        console.error("Failed to copy error:", err);
      });
  };

  const createGitHubIssueUrl = () => {
    const userAgent = navigator.userAgent;

    // Detect device type
    const isMobile =
      /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(
        userAgent,
      );
    const isTablet = /iPad|Android(?!.*Mobile)/i.test(userAgent);
    const deviceType = isTablet ? "Tablet" : isMobile ? "Mobile" : "Desktop";

    // Detect OS
    let os = "Unknown";
    if (userAgent.includes("Win")) os = "Windows";
    else if (userAgent.includes("Mac")) os = "macOS";
    else if (userAgent.includes("Linux")) os = "Linux";
    else if (userAgent.includes("Android")) os = "Android";
    else if (
      userAgent.includes("iOS") ||
      userAgent.includes("iPhone") ||
      userAgent.includes("iPad")
    )
      os = "iOS";

    // Detect browser
    let browser = "Unknown";
    if (userAgent.includes("Firefox")) browser = "Firefox";
    else if (userAgent.includes("Edg")) browser = "Edge";
    else if (userAgent.includes("Chrome")) browser = "Chrome";
    else if (userAgent.includes("Safari")) browser = "Safari";
    else if (userAgent.includes("Opera") || userAgent.includes("OPR"))
      browser = "Opera";

    const issueTitle = encodeURIComponent(
      title || "Error loading language data",
    );
    const issueBody = encodeURIComponent(
      `## Additional Context\n\n[Please describe what you were doing when this error occurred]\n\n## Device Information\n\n- Device Type: ${deviceType}\n- OS: ${os}\n- Browser: ${browser}\n- User Agent: \`${userAgent}\`\n\n## Error Details\n\n\`\`\`\n${message}\n\`\`\`\n\n`,
    );
    return `https://github.com/yaptown/yap/issues/new?title=${issueTitle}&body=${issueBody}&labels[]=bug`;
  };

  return (
    <div
      className={cn(
        "p-3 bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg overflow-hidden flex flex-col gap-2",
        className,
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <p
          className={cn(
            "text-sm text-red-600 dark:text-red-400 flex-1 line-clamp-3 break-all min-w-0",
            variant === "compact" ? "line-clamp-3" : "line-clamp-5",
          )}
        >
          {message}
        </p>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0 flex-shrink-0"
          onClick={handleCopy}
          title={copied ? "Copied!" : "Copy full error message"}
        >
          {copied ? (
            <Check className="h-3 w-3 text-green-600 dark:text-green-400" />
          ) : (
            <Copy className="h-3 w-3" />
          )}
        </Button>
      </div>

      {title && (
        <a
          href={createGitHubIssueUrl()}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center text-s gap-1"
        >
          <ExternalLink className="h-3 w-3" />
          Report on GitHub
        </a>
      )}
    </div>
  );
}
