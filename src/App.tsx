import { useCallback, useEffect, useRef, useState } from "react";
import Recorder from "./components/Recorder";
import SessionView from "./components/SessionView";
import Spinner from "./components/Spinner";
import {
  deleteSession,
  listSessions,
  reconcileOrphanSessions,
  searchSessions,
  type Session,
} from "./lib/db";
import { deleteRecording } from "./lib/recorder";

function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

/// FTS5 interpreta la sintassi delle query: una virgoletta o un operatore
/// digitati per caso farebbero fallire la ricerca invece di cercare.
function toMatchQuery(input: string): string {
  return input
    .split(/\s+/)
    .map((word) => word.replace(/"/g, "").trim())
    .filter((word) => word.length > 0)
    .map((word) => `"${word}"*`)
    .join(" AND ");
}

export default function App() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    const trimmed = query.trim();
    const load = trimmed
      ? searchSessions(toMatchQuery(trimmed))
      : listSessions();
    load.then(setSessions).catch((cause: unknown) => setError(String(cause)));
  }, [query]);

  // Prima di mostrare qualsiasi cosa si chiudono le sessioni rimaste aperte da
  // un'uscita brusca, altrimenti restano in lista come "Registrazione in corso".
  useEffect(() => {
    reconcileOrphanSessions()
      .catch(() => undefined)
      .finally(() => setReady(true));
  }, []);

  useEffect(() => {
    if (ready) refresh();
  }, [ready, refresh]);

  const searchInput = useRef<HTMLInputElement | null>(null);

  // ⌘F porta alla ricerca, Esc la svuota o torna al registratore.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.metaKey && event.key === "f") {
        event.preventDefault();
        searchInput.current?.focus();
        searchInput.current?.select();
      } else if (event.key === "Escape") {
        if (query) setQuery("");
        else setSelectedId(null);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [query]);

  const selected = sessions.find((session) => session.id === selectedId) ?? null;
  const totalMs = sessions.reduce(
    (sum, session) => sum + session.duration_ms,
    0,
  );

  async function removeSelected(session: Session) {
    if (session.audio_path) {
      await deleteRecording(session.audio_path).catch(() => undefined);
    }
    await deleteSession(session.id);
    setSelectedId(null);
    refresh();
  }

  if (!ready) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3">
        <Spinner size="md" />
        <span className="text-xs text-ink-muted">Apertura archivio…</span>
      </div>
    );
  }

  return (
    <div className="flex h-full">
      <aside className="flex w-72 shrink-0 flex-col border-r border-edge">
        <header className="space-y-3 px-4 pb-3 pt-10">
          <div className="flex items-center justify-between">
            <h1 className="text-sm font-semibold tracking-tight">Brief</h1>
            <button
              onClick={() => setSelectedId(null)}
              className="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white hover:opacity-90"
            >
              Nuova
            </button>
          </div>
          <input
            ref={searchInput}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Cerca nelle trascrizioni"
            className="w-full rounded-md border border-edge bg-surface-raised px-2.5 py-1.5 text-xs outline-none placeholder:text-ink-muted"
          />
        </header>

        <nav className="flex-1 overflow-y-auto px-2 pb-2">
          {sessions.length === 0 && (
            <p className="px-2 py-8 text-center text-xs leading-relaxed text-ink-muted">
              {query.trim()
                ? "Nessun risultato."
                : "Nessuna sessione registrata."}
            </p>
          )}
          {sessions.map((session) => (
            <button
              key={session.id}
              onClick={() => setSelectedId(session.id)}
              className={`w-full rounded-md px-2 py-2 text-left transition-colors ${
                session.id === selectedId
                  ? "bg-surface-raised"
                  : "hover:bg-surface-raised/60"
              }`}
            >
              <span className="block truncate text-sm">{session.title}</span>
              <span className="block text-xs text-ink-muted">
                {new Date(session.started_at).toLocaleDateString("it-IT", {
                  day: "numeric",
                  month: "short",
                })}{" "}
                · {formatDuration(session.duration_ms)}
              </span>
            </button>
          ))}
        </nav>

        {sessions.length > 0 && (
          <footer className="border-t border-edge px-4 py-2.5 text-[11px] text-ink-muted">
            {sessions.length}{" "}
            {sessions.length === 1 ? "sessione" : "sessioni"} ·{" "}
            {Math.round(totalMs / 60000)} min registrati
          </footer>
        )}
      </aside>

      <main className="flex-1 overflow-hidden pt-6">
        {selected ? (
          <SessionView
            session={selected}
            onChanged={refresh}
            onDelete={() => removeSelected(selected)}
          />
        ) : (
          <Recorder
            onFinished={(sessionId) => {
              refresh();
              setSelectedId(sessionId);
            }}
          />
        )}
      </main>

      {error && (
        <p className="absolute bottom-4 left-4 rounded-md border border-edge bg-surface-raised px-3 py-2 text-xs text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
