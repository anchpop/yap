import { Link } from "react-router-dom";

export function NotFoundPage() {
  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh] p-4 gap-4 text-center">
      <h1 className="text-6xl font-bold">404</h1>
      <p className="text-xl text-muted-foreground">Page not found</p>
      <Link
        to="/"
        className="text-primary underline underline-offset-4 hover:text-primary/80"
      >
        Go home
      </Link>
    </div>
  );
}
