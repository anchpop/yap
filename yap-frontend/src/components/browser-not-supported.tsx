export function BrowserNotSupported() {
  // Detect if user is on Safari
  const isSafari = /^((?!chrome|android).)*safari/i.test(navigator.userAgent);

  return (
    <div>
      <div className="min-h-screen bg-background flex items-center justify-center p-4">
        <div className="max-w-md w-full text-center space-y-6">
          <div className="space-y-2">
            <div className="w-16 h-16 bg-orange-100 dark:bg-orange-900/20 rounded-full flex items-center justify-center mx-auto">
              <svg
                className="w-8 h-8 text-orange-600 dark:text-orange-400"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                />
              </svg>
            </div>
            <h1 className="text-2xl font-semibold">Browser Not Supported</h1>
            <p className="text-muted-foreground">
              Yap requires modern browser features that aren't available in your current browser.
            </p>
          </div>

          {isSafari && (
            <div className="bg-yellow-100 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-4 text-left space-y-2">
              <p className="text-sm font-semibold text-yellow-800 dark:text-yellow-200 mb-1">
                Apple Users: Update Required
              </p>
              <p className="text-sm">
                On Safari, Yap.Town requires iOS 26 or macOS 26. (These versions are currently in beta.)
              </p>
              <p className="text-sm ">
                If you're on a desktop platform, you can also use Chrome or Firefox.
              </p>
            </div>
          )}

          <div className="bg-muted/50 rounded-lg p-4 text-left space-y-3">
            <p className="text-sm font-medium">Please use one of these browsers:</p>
            <ul className="space-y-2 text-sm">
              <li className="flex items-center gap-2">
                <span className="text-green-500">✓</span>
                <span>Google Chrome (version 86+)</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-green-500">✓</span>
                <span>Microsoft Edge (version 86+)</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-green-500">✓</span>
                <span>Safari (version 26.0+ - requires iOS/macOS 18, currently in beta)</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-green-500">✓</span>
                <span>Firefox (version 111+)</span>
              </li>
            </ul>
          </div>

          <div className="text-xs text-muted-foreground">
            <p>Yap uses the Origin Private File System (OPFS) API to store your learning data and work offline, some parts of which have <a href="https://developer.mozilla.org/en-US/docs/Web/API/FileSystemWritableFileStream" target="_blank" rel="noopener noreferrer">only recently been implemented in Safari</a>.</p>
          </div>
        </div>
      </div>
    </div>
  )
}
