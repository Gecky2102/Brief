import { useCallback, useEffect, useState } from "react";
import Recorder from "./components/Recorder";
import { listSessions, type Session } from "./lib/db";

function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export default function App() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listSessions()
      .then(setSessions)
      .catch((cause: unknown) => setError(String(cause)));
  }, []);

  useEffect(refresh, [refresh]);

  return (
    <div className="flex h-full">
      <aside className="flex w-72 shrink-0 flex-col border-r border-edge">
        <header className="flex items-center justify-between px-4 py-3">
          <h1 className="text-sm font-semibold tracking-tight">Brief</h1>
          <span className="text-xs text-ink-muted">{sessions.length}</span>
        </header>

        <nav className="flex-1 overflow-y-auto px-2 pb-2">
          {sessions.length === 0 && !error && (
            <p className="px-2 py-8 text-center text-xs text-ink-muted">
              Nessuna sessione registrata.
            </p>
          )}
          {sessions.map((session) => (
            <button
              key={session.id}
              className="w-full rounded-md px-2 py-2 text-left hover:bg-surface-raised"
            >
              <span className="block truncate text-sm">{session.title}</span>
              <span className="block text-xs text-ink-muted">
                {formatDuration(session.duration_ms)}
              </span>
            </button>
          ))}
        </nav>
      </aside>

      <main className="flex flex-1 flex-col items-center justify-center gap-6 p-8">
        <Recorder onFinished={refresh} />
        {error && (
          <p className="max-w-md text-center text-sm text-red-400">{error}</p>
        )}
      </main>
    </div>
  );
}
