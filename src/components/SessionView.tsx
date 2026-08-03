import { useCallback, useEffect, useState } from "react";
import Spinner from "./Spinner";
import ReportView from "./ReportView";
import {
  KIND_LABELS,
  listSegments,
  loadAnalysis,
  saveAnalysis,
  setSessionKind,
  setSessionTitle,
  updateSegmentText,
  type Analysis,
  type Segment,
  type Session,
  type SessionKind,
} from "../lib/db";
import { fileNameFor, speakerOf, toMarkdown } from "../lib/markdown";
import {
  analyzeSession,
  exportAudio,
  exportMarkdown,
  onAnalysisProgress,
  onDownloadProgress,
  type AnalysisProgress,
  type DownloadProgress,
} from "../lib/recorder";

type Props = {
  session: Session;
  onChanged: () => void;
  onDelete: () => void;
};

function formatStamp(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export default function SessionView({ session, onChanged, onDelete }: Props) {
  const [segments, setSegments] = useState<Segment[]>([]);
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [progress, setProgress] = useState<AnalysisProgress | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [title, setTitle] = useState(session.title);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [tab, setTab] = useState<"report" | "transcript">("report");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [draft, setDraft] = useState("");

  const load = useCallback(() => {
    setTitle(session.title);
    setError(null);
    setNotice(null);
    setConfirmingDelete(false);
    listSegments(session.id).then(setSegments).catch(() => undefined);
    loadAnalysis(session.id).then(setAnalysis).catch(() => undefined);
  }, [session.id, session.title]);

  useEffect(load, [load]);

  useEffect(() => {
    const subscriptions = [
      onDownloadProgress((event) =>
        setDownload(event.downloaded >= event.total ? null : event),
      ),
      onAnalysisProgress(setProgress),
    ];
    return () => {
      subscriptions.forEach((s) => s.then((unlisten) => unlisten()));
    };
  }, []);

  async function runAnalysis() {
    setAnalyzing(true);
    setProgress(null);
    setError(null);
    try {
      const lines = segments.map((segment) => ({
        speaker: speakerOf(segment.track),
        text: segment.text,
      }));
      const result = await analyzeSession(lines);
      await saveAnalysis(session.id, result, "provider-online");
      setAnalysis(result);

      // Il tipo proposto dall'IA viene applicato, ma resta modificabile: la
      // scelta finale è dell'utente.
      if (result.kind !== "unknown") {
        await setSessionKind(session.id, result.kind);
      }
      if (result.title.trim()) {
        await setSessionTitle(session.id, result.title.trim());
        setTitle(result.title.trim());
      }
      onChanged();
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setAnalyzing(false);
      setProgress(null);
      setDownload(null);
    }
  }

  async function commitSegment(segment: Segment) {
    const trimmed = draft.trim();
    setEditingId(null);
    if (!trimmed || trimmed === segment.text) return;
    await updateSegmentText(segment.id, trimmed);
    setSegments((current) =>
      current.map((item) =>
        item.id === segment.id ? { ...item, text: trimmed } : item,
      ),
    );
  }

  async function changeKind(kind: SessionKind) {
    await setSessionKind(session.id, kind);
    onChanged();
  }

  async function commitTitle() {
    const trimmed = title.trim();
    if (!trimmed || trimmed === session.title) return;
    await setSessionTitle(session.id, trimmed);
    onChanged();
  }

  async function saveMarkdown() {
    setError(null);
    try {
      const contents = toMarkdown({ ...session, title }, segments, analysis);
      const saved = await exportMarkdown(fileNameFor(session), contents);
      if (saved) setNotice("Trascrizione esportata.");
    } catch (cause: unknown) {
      setError(String(cause));
    }
  }

  async function copyToClipboard() {
    setError(null);
    try {
      const contents = toMarkdown({ ...session, title }, segments, analysis);
      await navigator.clipboard.writeText(contents);
      setNotice("Copiato negli appunti.");
    } catch {
      setError("Copia negli appunti non riuscita.");
    }
  }

  async function saveAudio() {
    setError(null);
    if (!session.audio_path) {
      setError("Questa sessione non ha file audio.");
      return;
    }
    try {
      const saved = await exportAudio(session.audio_path);
      if (saved) setNotice("Audio esportato.");
    } catch (cause: unknown) {
      setError(String(cause));
    }
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <header className="brief-drag space-y-3 border-b border-edge px-8 pb-5 pt-12">
        <input
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          onBlur={commitTitle}
          className="w-full bg-transparent text-[22px] font-semibold tracking-tight outline-none"
        />
        <div className="flex flex-wrap items-center gap-3 text-xs text-ink-muted">
          <span>{new Date(session.started_at).toLocaleString("it-IT")}</span>
          <span>·</span>
          <span>{formatStamp(session.duration_ms)}</span>
          <span>·</span>
          <select
            value={session.kind}
            onChange={(event) => changeKind(event.target.value as SessionKind)}
            className="brief-field px-2 py-1 text-xs text-ink"
          >
            {Object.entries(KIND_LABELS).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </div>

        <div className="flex flex-wrap gap-2 pt-1">
          <button
            onClick={runAnalysis}
            disabled={analyzing || segments.length === 0}
            className="brief-button-primary px-3 py-1.5 text-xs disabled:opacity-40"
          >
            {analyzing ? (
              <Spinner label="Scrittura in corso…" />
            ) : analysis ? (
              "Rigenera report"
            ) : (
              "Genera report"
            )}
          </button>
          <button
            onClick={copyToClipboard}
            className="brief-button px-3 py-1.5 text-xs"
          >
            Copia
          </button>
          <button
            onClick={saveMarkdown}
            className="brief-button px-3 py-1.5 text-xs"
          >
            Esporta Markdown
          </button>
          <button
            onClick={saveAudio}
            className="brief-button px-3 py-1.5 text-xs"
          >
            Esporta audio
          </button>
          {confirmingDelete ? (
            <span className="flex items-center gap-2 rounded-md border border-live/40 bg-live/10 px-3 py-1.5 text-xs">
              <span className="text-ink-muted">Eliminare sessione e audio?</span>
              <button onClick={onDelete} className="font-medium text-live">
                Elimina
              </button>
              <button
                onClick={() => setConfirmingDelete(false)}
                className="text-ink-muted hover:text-ink"
              >
                Annulla
              </button>
            </span>
          ) : (
            <button
              onClick={() => setConfirmingDelete(true)}
              className="brief-button px-3 py-1.5 text-xs text-ink-muted hover:text-live"
            >
              Elimina
            </button>
          )}
        </div>

        {download && (
          <div className="space-y-1.5 pt-2">
            <p className="text-xs text-ink-muted">
              {download.label}: {(download.downloaded / 1048576).toFixed(0)} MB di{" "}
              {(download.total / 1048576).toFixed(0)} MB
            </p>
            <div className="h-1.5 overflow-hidden rounded-full bg-surface-raised">
              <div
                className="h-full bg-accent"
                style={{
                  width: `${Math.round((download.downloaded / download.total) * 100)}%`,
                }}
              />
            </div>
          </div>
        )}

        {notice && <p className="text-xs text-ink-muted">{notice}</p>}
        {error && <p className="rounded-md border border-live/40 bg-live/10 px-3 py-2 text-xs leading-relaxed text-live">{error}</p>}
      </header>

      <div className="flex gap-1 border-b border-edge px-8">
        {(["report", "transcript"] as const).map((value) => (
          <button
            key={value}
            onClick={() => setTab(value)}
            className={`-mb-px border-b-2 px-3 py-2 text-xs transition-colors ${
              tab === value
                ? "border-accent font-medium text-ink"
                : "border-transparent text-ink-muted hover:text-ink"
            }`}
          >
            {value === "report" ? "Report" : "Trascrizione"}
          </button>
        ))}
      </div>

      <div className="space-y-8 px-8 py-6">
        {analyzing && (
          <div className="space-y-3 rounded-xl border border-edge bg-surface-raised/40 p-5">
            <div className="flex items-center justify-between text-xs text-ink-muted">
              <span className="flex items-center gap-2">
                <Spinner />
                {progress?.phase === "reading"
                  ? `Lettura della trascrizione, parte ${progress.step + 1} di ${progress.steps - 1}`
                  : "Scrittura del riassunto"}
              </span>
              {progress && progress.steps > 1 && (
                <span className="font-mono">
                  {Math.round(((progress.step + 1) / progress.steps) * 100)}%
                </span>
              )}
            </div>

            {progress && progress.steps > 1 && (
              <div className="h-1 overflow-hidden rounded-full bg-surface-sunken">
                <div
                  className="h-full rounded-full bg-accent transition-[width]"
                  style={{
                    width: `${Math.round(((progress.step + 1) / progress.steps) * 100)}%`,
                  }}
                />
              </div>
            )}

            {progress?.preview ? (
              <p className="max-h-52 overflow-y-auto whitespace-pre-wrap text-xs leading-relaxed text-ink-muted">
                {progress.preview}
              </p>
            ) : (
              <>
                <div className="brief-skeleton h-3 w-full rounded" />
                <div className="brief-skeleton h-3 w-5/6 rounded" />
              </>
            )}
          </div>
        )}

        {tab === "report" &&
          (analysis?.report ? (
            <ReportView markdown={analysis.report} />
          ) : (
            !analyzing && (
              <p className="mx-auto max-w-md py-10 text-center text-xs leading-relaxed text-ink-muted">
                Nessun report per questa sessione. Premi «Genera report» per
                produrre un documento completo a partire dalla trascrizione.
              </p>
            )
          ))}


        {tab === "transcript" && (
        <section className="space-y-3">
          {segments.length === 0 ? (
            <p className="text-sm text-ink-muted">
              Nessun parlato riconosciuto in questa sessione.
            </p>
          ) : (
            segments.map((segment) => (
              <div
                key={segment.id}
                className="group flex gap-3 rounded-md px-2 py-1 -mx-2 hover:bg-surface-raised/50"
              >
                <span className="shrink-0 pt-0.5 font-mono text-xs text-ink-muted">
                  {formatStamp(segment.start_ms)}
                </span>
                <span
                  className={`w-24 shrink-0 pt-0.5 text-xs font-medium ${
                    segment.track === "mic" ? "text-accent" : "text-ink-muted"
                  }`}
                >
                  {speakerOf(segment.track)}
                </span>
                {editingId === segment.id ? (
                  <textarea
                    autoFocus
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    onBlur={() => commitSegment(segment)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        void commitSegment(segment);
                      } else if (event.key === "Escape") {
                        setEditingId(null);
                      }
                    }}
                    rows={2}
                    className="flex-1 rounded border border-accent bg-surface-sunken px-2 py-1 text-sm leading-relaxed outline-none"
                  />
                ) : (
                  <p
                    onClick={() => {
                      setEditingId(segment.id);
                      setDraft(segment.text);
                    }}
                    title="Clicca per correggere"
                    className="flex-1 cursor-text text-sm leading-relaxed"
                  >
                    {segment.text}
                  </p>
                )}
              </div>
            ))
          )}
        </section>
        )}
      </div>
    </div>
  );
}
