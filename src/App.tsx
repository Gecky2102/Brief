import { useCallback, useEffect, useRef, useState } from "react";
import Recorder from "./components/Recorder";
import SessionView from "./components/SessionView";
import SettingsPanel from "./components/SettingsPanel";
import Shortcuts from "./components/Shortcuts";
import Spinner from "./components/Spinner";
import {
  deleteSession,
  listSessions,
  reconcileOrphanSessions,
  searchWithExcerpts,
  type SearchHit,
  type Session,
} from "./lib/db";
import { deleteRecording, exportMany } from "./lib/recorder";
import {
  createFolder,
  deleteFolder,
  listFolders,
  listSegments,
  loadAnalysis,
  type Folder,
} from "./lib/db";
import { fileNameFor, toMarkdown } from "./lib/markdown";

/// Raggruppa per periodo come fanno Foto e Note: scorrere una lista piatta di
/// cento sessioni è scomodo.
function periodOf(iso: string): string {
  const data = new Date(iso);
  const oggi = new Date();
  const ieri = new Date(oggi);
  ieri.setDate(oggi.getDate() - 1);

  const stessoGiorno = (a: Date, b: Date) =>
    a.toDateString() === b.toDateString();

  if (stessoGiorno(data, oggi)) return "Oggi";
  if (stessoGiorno(data, ieri)) return "Ieri";

  const giorniFa = (oggi.getTime() - data.getTime()) / 86400000;
  if (giorniFa < 7) return "Ultimi sette giorni";
  if (data.getFullYear() === oggi.getFullYear()) {
    return data.toLocaleDateString("it-IT", { month: "long" });
  }
  return data.toLocaleDateString("it-IT", { month: "long", year: "numeric" });
}

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
  const [hits, setHits] = useState<Record<number, SearchHit>>({});
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [ready, setReady] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [folders, setFolders] = useState<(Folder & { count: number })[]>([]);
  const [activeFolder, setActiveFolder] = useState<number | null | undefined>(
    undefined,
  );
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setHits({});
      listSessions(activeFolder)
        .then(setSessions)
        .catch((cause: unknown) => setError(String(cause)));
      listFolders().then(setFolders).catch(() => undefined);
      return;
    }

    searchWithExcerpts(toMatchQuery(trimmed))
      .then((risultati) => {
        setSessions(risultati);
        setHits(
          Object.fromEntries(risultati.map((hit) => [hit.id, hit])),
        );
      })
      .catch((cause: unknown) => setError(String(cause)));
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
      if (event.metaKey && event.key === ",") {
        event.preventDefault();
        setShowSettings(true);
      } else if (event.metaKey && event.key === "n") {
        event.preventDefault();
        setShowSettings(false);
        setSelectedId(null);
      } else if (event.metaKey && event.key === "f") {
        event.preventDefault();
        searchInput.current?.focus();
        searchInput.current?.select();
      } else if (event.key === "?" && !event.metaKey) {
        const target = event.target as HTMLElement | null;
        if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
        event.preventDefault();
        setShowShortcuts(true);
      } else if (event.key === "Escape") {
        if (showShortcuts) setShowShortcuts(false);
        else if (query) setQuery("");
        else setSelectedId(null);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [query, showShortcuts]);

  const selected = sessions.find((session) => session.id === selectedId) ?? null;
  const totalMs = sessions.reduce(
    (sum, session) => sum + session.duration_ms,
    0,
  );

  /// Esporta in blocco quanto è attualmente in elenco: con la ricerca attiva
  /// diventa «esporta tutto ciò che parla di X».
  async function esportaTutte() {
    setExporting(true);
    try {
      const documenti: [string, string][] = [];
      for (const session of sessions) {
        const [segments, analysis] = await Promise.all([
          listSegments(session.id),
          loadAnalysis(session.id),
        ]);
        documenti.push([
          fileNameFor(session),
          toMarkdown(session, segments, analysis),
        ]);
      }
      await exportMany(documenti);
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setExporting(false);
    }
  }

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
      <aside className="flex w-64 shrink-0 flex-col border-r border-edge">
        <header className="brief-drag space-y-2.5 px-3 pb-2.5 pt-11">
          <div className="flex items-center justify-between">
            <h1 className="text-[13px] font-semibold tracking-tight">Brief</h1>
            <div className="flex items-center gap-1.5">
              <button
                onClick={() => setShowSettings(true)}
                title="Impostazioni"
                className="brief-button px-2 py-1 text-xs text-ink-muted hover:text-ink"
              >
                ⚙
              </button>
              <button
                onClick={() => {
                  setShowSettings(false);
                  setSelectedId(null);
                }}
                className="brief-button-primary px-2.5 py-1 text-xs"
              >
                Nuova
              </button>
            </div>
          </div>
          <input
            ref={searchInput}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Cerca nelle trascrizioni"
            className="brief-field w-full px-2.5 py-1.5 text-xs"
          />
        </header>

        {folders.length > 0 && (
          <div className="flex flex-wrap gap-1 px-3 pb-2">
            <button
              onClick={() => setActiveFolder(undefined)}
              className={`rounded-full px-2 py-0.5 text-[11px] transition-colors ${
                activeFolder === undefined
                  ? "bg-accent text-white"
                  : "border border-edge hover:bg-surface-raised"
              }`}
            >
              Tutte
            </button>
            {folders.map((folder) => (
              <button
                key={folder.id}
                onClick={() => setActiveFolder(folder.id)}
                onContextMenu={async (event) => {
                  event.preventDefault();
                  await deleteFolder(folder.id);
                  if (activeFolder === folder.id) setActiveFolder(undefined);
                  refresh();
                }}
                title="Tasto destro per eliminare la cartella, le sessioni restano"
                className={`rounded-full px-2 py-0.5 text-[11px] transition-colors ${
                  activeFolder === folder.id
                    ? "bg-accent text-white"
                    : "border border-edge hover:bg-surface-raised"
                }`}
              >
                {folder.name} {folder.count}
              </button>
            ))}
            <button
              onClick={async () => {
                const nome = window.prompt("Nome della cartella");
                if (nome?.trim()) {
                  await createFolder(nome.trim());
                  refresh();
                }
              }}
              className="rounded-full border border-edge px-2 py-0.5 text-[11px] text-ink-muted hover:bg-surface-raised"
            >
              +
            </button>
          </div>
        )}

        {folders.length === 0 && sessions.length > 2 && (
          <button
            onClick={async () => {
              const nome = window.prompt("Nome della prima cartella");
              if (nome?.trim()) {
                await createFolder(nome.trim());
                refresh();
              }
            }}
            className="mx-3 mb-2 rounded-md border border-edge px-2 py-1 text-[11px] text-ink-muted hover:bg-surface-raised"
          >
            Crea una cartella
          </button>
        )}

        <nav className="flex-1 overflow-y-auto px-2 pb-2">
          {sessions.length === 0 && (
            <p className="px-3 py-10 text-center text-xs leading-relaxed text-ink-muted">
              {query.trim() ? (
                <>
                  Nessuna riga contiene «{query.trim()}».
                  <br />
                  La ricerca guarda dentro le trascrizioni, non solo nei titoli.
                </>
              ) : (
                <>
                  Nessuna sessione.
                  <br />
                  Registra qualcosa oppure importa un file audio.
                </>
              )}
            </p>
          )}
          {sessions.map((session, indice) => {
            const periodo = periodOf(session.started_at);
            const nuovoPeriodo =
              indice === 0 || periodOf(sessions[indice - 1].started_at) !== periodo;
            return (
              <div key={session.id}>
                {nuovoPeriodo && (
                  <span className="mt-3 mb-1 block px-2 text-[11px] font-medium uppercase tracking-wide text-ink-muted first:mt-1">
                    {periodo}
                  </span>
                )}
            <button
              onClick={() => {
                setShowSettings(false);
                setSelectedId(session.id);
              }}
              className={`w-full rounded-md px-2.5 py-1.5 text-left transition-colors ${
                session.id === selectedId
                  ? "bg-accent text-white"
                  : "hover:bg-surface-raised"
              }`}
            >
              <span className="block truncate text-[13px]">{session.title}</span>
              <span
                className={`block text-[11px] ${
                  session.id === selectedId ? "text-white/70" : "text-ink-muted"
                }`}
              >
                {new Date(session.started_at).toLocaleDateString("it-IT", {
                  day: "numeric",
                  month: "short",
                })}{" "}
                · {formatDuration(session.duration_ms)}
                {hits[session.id] && ` · ${hits[session.id].hits} risultati`}
              </span>
              {hits[session.id]?.excerpt && (
                <span
                  className={`mt-0.5 block truncate text-[11px] ${
                    session.id === selectedId ? "text-white/70" : "text-ink-muted"
                  }`}
                >
                  {hits[session.id].excerpt}
                </span>
              )}
            </button>
              </div>
            );
          })}
        </nav>

        {sessions.length > 0 && (
          <footer className="flex items-center justify-between gap-2 border-t border-edge px-3 py-2 text-[11px] text-ink-muted">
            <span>
              {sessions.length}{" "}
              {sessions.length === 1 ? "sessione" : "sessioni"} ·{" "}
              {Math.round(totalMs / 60000)} min
            </span>
            <button
              onClick={esportaTutte}
              disabled={exporting}
              className="brief-button px-2 py-0.5 text-[11px] disabled:opacity-40"
              title="Salva tutte le sessioni in elenco come file Markdown"
            >
              {exporting ? "Esporto…" : "Esporta"}
            </button>
          </footer>
        )}
      </aside>

      <main className="brief-content flex-1 overflow-hidden">
        {showSettings ? (
          <SettingsPanel onClose={() => setShowSettings(false)} />
        ) : selected ? (
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

      {showShortcuts && <Shortcuts onClose={() => setShowShortcuts(false)} />}

      {error && (
        <p className="absolute bottom-4 left-4 rounded-md border border-edge bg-surface-raised px-3 py-2 text-xs text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
