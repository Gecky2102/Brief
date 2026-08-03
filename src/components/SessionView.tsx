import { useCallback, useEffect, useState } from "react";
import Spinner from "./Spinner";
import ReportView from "./ReportView";
import SpeakerBar, { speakerColor } from "./SpeakerBar";
import AudioPlayer from "./AudioPlayer";
import SpeakerStats from "./SpeakerStats";
import {
  KIND_LABELS,
  listSegments,
  listSpeakers,
  loadAnalysis,
  mergeSpeakers,
  renameSpeaker,
  saveAnalysis,
  setSessionKind,
  setSessionTitle,
  updateSegmentText,
  type Analysis,
  type Segment,
  type Session,
  type Speaker,
  type SessionKind,
} from "../lib/db";
import { REPORT_LENGTHS, REPORT_STYLES } from "../lib/catalog";
import { fileNameFor, speakerOf, toMarkdown } from "../lib/markdown";
import {
  analyzeSession,
  audioFile,
  getSettings,
  setSettings,
  exportAudio,
  exportMarkdown,
  onAnalysisProgress,
  onDownloadProgress,
  type AnalysisProgress,
  type DownloadProgress,
  type ReportLength,
  type ReportStyle,
  type Settings,
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

/// Unisce le righe consecutive della stessa voce: la trascrizione a finestre
/// spezza gli interventi ogni pochi secondi, e leggerli separati è faticoso.
function groupBySpeaker(segments: Segment[]): Segment[][] {
  const gruppi: Segment[][] = [];
  for (const segment of segments) {
    const ultimo = gruppi[gruppi.length - 1];
    const stessaVoce =
      ultimo &&
      ultimo[0].track === segment.track &&
      ultimo[0].speaker_id === segment.speaker_id &&
      segment.start_ms - ultimo[ultimo.length - 1].end_ms < 4000;

    if (stessaVoce) {
      ultimo.push(segment);
    } else {
      gruppi.push([segment]);
    }
  }
  return gruppi;
}

export default function SessionView({ session, onChanged, onDelete }: Props) {
  const [segments, setSegments] = useState<Segment[]>([]);
  const [speakers, setSpeakers] = useState<Speaker[]>([]);
  const [audioPath, setAudioPath] = useState<string | null>(null);
  const [seekTo, setSeekTo] = useState<number | null>(null);
  const [filter, setFilter] = useState("");
  const [settings, setLocalSettings] = useState<Settings | null>(null);
  const [showOptions, setShowOptions] = useState(false);
  const [playhead, setPlayhead] = useState(0);
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
    listSpeakers(session.id).then(setSpeakers).catch(() => undefined);
    getSettings().then(setLocalSettings).catch(() => undefined);
    setAudioPath(null);
    setPlayhead(0);
    if (session.audio_path) {
      audioFile(session.audio_path).then(setAudioPath).catch(() => undefined);
    }
    loadAnalysis(session.id).then(setAnalysis).catch(() => undefined);
  }, [session.id, session.title]);

  useEffect(load, [load]);

  // ⌘R rigenera, ⌘P stampa, ⌘⇧C copia: le tre azioni che si ripetono di più.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (!event.metaKey) return;
      if (event.key === "r") {
        event.preventDefault();
        if (!analyzing && segments.length > 0) void runAnalysis();
      } else if (event.key === "p") {
        event.preventDefault();
        if (analysis?.report) stampaReport();
      } else if (event.shiftKey && event.key.toLowerCase() === "c") {
        event.preventDefault();
        void copyToClipboard();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

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

  /// Il taglio si cambia da qui e vale subito per la rigenerazione: passare
  /// dalle impostazioni per provare un altro formato era macchinoso.
  async function cambiaOpzione(patch: Partial<Settings>) {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setLocalSettings(next);
    await setSettings(next).catch(() => undefined);
  }

  async function runAnalysis() {
    setAnalyzing(true);
    setProgress(null);
    setError(null);
    try {
      // I nomi dati alle voci entrano nel report: senza, il modello non può
      // attribuire a nessuno ciò che viene detto.
      const lines = segments.map((segment) => ({
        speaker: speakerOf(segment.track, segment.speaker_label),
        text: segment.text,
      }));
      const nomi = [
        ...new Set(
          segments.map((segment) =>
            speakerOf(segment.track, segment.speaker_label),
          ),
        ),
      ];

      const result = await analyzeSession(lines, {
        date: new Date(session.started_at).toLocaleString("it-IT", {
          dateStyle: "full",
          timeStyle: "short",
        }),
        duration_minutes: Math.round(session.duration_ms / 60000),
        speakers: nomi,
      });
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

  const ricaricaVoci = useCallback(() => {
    listSegments(session.id).then(setSegments).catch(() => undefined);
    listSpeakers(session.id).then(setSpeakers).catch(() => undefined);
  }, [session.id]);

  async function rinomina(id: number, label: string) {
    await renameSpeaker(id, label);
    ricaricaVoci();
  }

  async function unisci(from: number, into: number) {
    await mergeSpeakers(from, into);
    ricaricaVoci();
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

  function stampaReport() {
    // La stampa di sistema di macOS offre «Salva come PDF»: è il modo più
    // diretto per avere il documento impaginato senza un motore PDF a bordo.
    window.print();
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
              "Rigenera report ⌘R"
            ) : (
              "Genera report ⌘R"
            )}
          </button>
          <button
            onClick={() => setShowOptions((aperto) => !aperto)}
            className="brief-button px-3 py-1.5 text-xs"
          >
            Taglio {showOptions ? "▾" : "▸"}
          </button>
          <button
            onClick={stampaReport}
            disabled={!analysis?.report}
            className="brief-button px-3 py-1.5 text-xs disabled:opacity-40"
          >
            Esporta PDF ⌘P
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

        {showOptions && settings && (
          <div className="space-y-2 rounded-xl border border-edge bg-surface-raised/60 p-3">
            <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
              {REPORT_STYLES.map((option) => (
                <button
                  key={option.value}
                  onClick={() =>
                    cambiaOpzione({ report_style: option.value as ReportStyle })
                  }
                  title={option.detail}
                  className={`rounded-lg border px-2 py-1.5 text-[11px] transition-colors ${
                    settings.report_style === option.value
                      ? "border-accent bg-accent-soft"
                      : "border-edge hover:bg-surface-sunken"
                  }`}
                >
                  {option.label}
                </button>
              ))}
            </div>
            <div className="flex gap-1.5">
              {REPORT_LENGTHS.map((option) => (
                <button
                  key={option.value}
                  onClick={() =>
                    cambiaOpzione({
                      report_length: option.value as ReportLength,
                    })
                  }
                  className={`flex-1 rounded-lg border px-2 py-1.5 text-[11px] transition-colors ${
                    settings.report_length === option.value
                      ? "border-accent bg-accent-soft"
                      : "border-edge hover:bg-surface-sunken"
                  }`}
                >
                  {option.label}
                </button>
              ))}
            </div>
            <p className="text-[11px] text-ink-muted">
              Premi «Rigenera report» per applicare il nuovo taglio.
            </p>
          </div>
        )}

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
        <section className="space-y-4">
          <SpeakerBar
            speakers={speakers}
            counts={segments.reduce<Record<number, number>>((acc, segment) => {
              if (segment.speaker_id !== null) {
                acc[segment.speaker_id] = (acc[segment.speaker_id] ?? 0) + 1;
              }
              return acc;
            }, {})}
            onRename={rinomina}
            onMerge={unisci}
          />

          <SpeakerStats segments={segments} speakers={speakers} />

          <input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="Filtra le righe della trascrizione"
            className="brief-field w-full px-3 py-1.5 text-xs"
          />

          {audioPath && (
            <AudioPlayer
              path={audioPath}
              seekTo={seekTo}
              onTime={setPlayhead}
            />
          )}
          {segments.length === 0 ? (
            <p className="text-sm text-ink-muted">
              Nessun parlato riconosciuto in questa sessione.
            </p>
          ) : (
            groupBySpeaker(
              segments.filter(
                (segment) =>
                  !filter.trim() ||
                  segment.text.toLowerCase().includes(filter.toLowerCase()) ||
                  (segment.speaker_label ?? "")
                    .toLowerCase()
                    .includes(filter.toLowerCase()),
              ),
            ).map((gruppo) => {
              const primo = gruppo[0];
              const attivo =
                playhead >= primo.start_ms &&
                playhead < gruppo[gruppo.length - 1].end_ms;
              const colore =
                primo.track === "mic"
                  ? "var(--accent)"
                  : primo.speaker_id !== null
                    ? speakerColor(
                        speakers.find((v) => v.id === primo.speaker_id)
                          ?.cluster_index ?? 0,
                      )
                    : "var(--ink-muted)";

              return (
                <div
                  key={primo.id}
                  className={`-mx-2 flex gap-3 rounded-md px-2 py-1.5 transition-colors ${
                    attivo ? "bg-accent-soft" : "hover:bg-surface-raised/50"
                  }`}
                >
                  <button
                    onClick={() => setSeekTo(primo.start_ms)}
                    title="Ascolta da qui"
                    disabled={!audioPath}
                    className="shrink-0 pt-0.5 font-mono text-xs text-ink-muted hover:text-accent disabled:hover:text-ink-muted"
                  >
                    {formatStamp(primo.start_ms)}
                  </button>

                  <div className="min-w-0 flex-1">
                    <span
                      className="mb-0.5 block text-xs font-medium"
                      style={{ color: colore }}
                    >
                      {speakerOf(primo.track, primo.speaker_label)}
                    </span>
                    {gruppo.map((segment) => (
                      <span key={segment.id}>
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
                            className="brief-field my-1 w-full px-2 py-1 text-sm leading-relaxed"
                          />
                        ) : (
                          <span
                            onClick={() => {
                              setEditingId(segment.id);
                              setDraft(segment.text);
                            }}
                            title="Clicca per correggere"
                            className="cursor-text text-sm leading-relaxed"
                          >
                            {segment.text}{" "}
                          </span>
                        )}
                      </span>
                    ))}
                  </div>
                </div>
              );
            })
          )}
        </section>
        )}
      </div>
    </div>
  );
}
