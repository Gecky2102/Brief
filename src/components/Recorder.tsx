import { useEffect, useRef, useState } from "react";
import LevelMeter from "./LevelMeter";
import Spinner from "./Spinner";
import { addSegment, createSession, finishSession } from "../lib/db";
import { speakerOf } from "../lib/markdown";
import {
  compressRecording,
  importAudio,
  modelsStatus,
  onImportProgress,
  onDownloadProgress,
  onLevel,
  onSegment,
  onTranscriptError,
  startRecording,
  stopRecording,
  systemTrackHealth,
  type DownloadProgress,
  type SegmentEvent,
} from "../lib/recorder";

type Props = {
  onFinished: (sessionId: number) => void;
};

type Phase = "idle" | "preparing" | "recording" | "finishing";

function formatClock(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function defaultTitle(at: Date): string {
  return `Sessione del ${at.toLocaleDateString("it-IT", {
    day: "numeric",
    month: "long",
  })}, ${at.toLocaleTimeString("it-IT", { hour: "2-digit", minute: "2-digit" })}`;
}

function formatSize(bytes: number): string {
  return `${(bytes / 1048576).toFixed(0)} MB`;
}

export default function Recorder({ onFinished }: Props) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [levels, setLevels] = useState({ mic: 0, system: 0 });
  const [elapsedMs, setElapsedMs] = useState(0);
  const [lines, setLines] = useState<SegmentEvent[]>([]);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [modelReady, setModelReady] = useState<boolean | null>(null);
  const [analysisReady, setAnalysisReady] = useState(true);
  const [systemWarning, setSystemWarning] = useState<string | null>(null);
  const [importing, setImporting] = useState<{
    done: number;
    total: number;
    eta: number;
  } | null>(null);
  const importStarted = useRef<number>(0);
  const [error, setError] = useState<string | null>(null);

  const startedAt = useRef<Date | null>(null);
  const sessionId = useRef<number | null>(null);
  const transcriptEnd = useRef<HTMLDivElement | null>(null);
  const lastSpeechMs = useRef<number>(0);

  const recording = phase === "recording";
  const busy = phase === "preparing" || phase === "finishing";

  useEffect(() => {
    modelsStatus()
      .then((status) => {
        setModelReady(status.transcription);
        setAnalysisReady(status.analysis);
      })
      .catch(() => setModelReady(false));
  }, []);

  useEffect(() => {
    const subscriptions = [
      onLevel((event) => {
        setLevels((current) => ({ ...current, [event.track]: event.rms }));
        if (event.rms > 0.015) lastSpeechMs.current = Date.now();
      }),
      onSegment((event) => {
        if (event.session_id !== sessionId.current) return;
        setLines((current) => [...current, event]);
        addSegment({
          sessionId: event.session_id,
          track: event.track,
          startMs: event.start_ms,
          endMs: event.end_ms,
          text: event.text,
          speaker: event.speaker,
        }).catch(() => undefined);
      }),
      onTranscriptError(setError),
      onImportProgress((event) => {
        // Stima il tempo mancante dal ritmo tenuto finora: whisper procede in
        // modo abbastanza regolare da renderla attendibile.
        const trascorso = Date.now() - importStarted.current;
        const frazione = event.done_ms / Math.max(event.total_ms, 1);
        const eta = frazione > 0.01 ? (trascorso / frazione) * (1 - frazione) : 0;
        setImporting({
          done: event.done_ms,
          total: event.total_ms,
          eta: Math.round(eta),
        });
      }),
      onDownloadProgress((event) => {
        setDownload(event.downloaded >= event.total ? null : event);
      }),
    ];
    return () => {
      subscriptions.forEach((subscription) =>
        subscription.then((unlisten) => unlisten()),
      );
    };
  }, []);

  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => {
      if (startedAt.current) {
        setElapsedMs(Date.now() - startedAt.current.getTime());
      }
    }, 500);
    return () => window.clearInterval(timer);
  }, [recording]);

  useEffect(() => {
    transcriptEnd.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines.length]);

  // La traccia di sistema fallisce in silenzio quando manca il permesso di
  // registrazione schermo: senza questo controllo l'utente registra a vuoto.
  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(async () => {
      const health = await systemTrackHealth().catch(() => 1);
      if (health < 0) {
        setSystemWarning(
          "L'audio di sistema non è attivo: manca il permesso di registrazione schermo. Concedilo in Impostazioni di Sistema › Privacy e sicurezza › Registrazione schermo, poi riavvia Brief. Il microfono viene registrato comunque.",
        );
      } else if (health === 0 && elapsedMs > 20000) {
        setSystemWarning(
          "Nessun audio di sistema rilevato finora. Se stai registrando una call, controlla che l'audio esca dagli altoparlanti o dalle cuffie del Mac.",
        );
      } else if (health > 0) {
        setSystemWarning(null);
      }
    }, 5000);
    return () => window.clearInterval(timer);
  }, [recording, elapsedMs]);

  // Barra spaziatrice per avviare e fermare, come in ogni registratore.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) {
        return;
      }
      if (event.code === "Space" && !busy) {
        event.preventDefault();
        void (recording ? end() : begin());
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  async function begin() {
    setPhase("preparing");
    setError(null);
    setLines([]);
    try {
      const begunAt = new Date();
      const id = await createSession(begunAt.toISOString());
      sessionId.current = id;
      await startRecording(id);
      startedAt.current = begunAt;
      lastSpeechMs.current = Date.now();
      setElapsedMs(0);
      setSystemWarning(null);
      setPhase("recording");
      setModelReady(true);
    } catch (cause: unknown) {
      setError(String(cause));
      sessionId.current = null;
      startedAt.current = null;
      setPhase("idle");
    } finally {
      setDownload(null);
    }
  }

  async function end() {
    setPhase("finishing");
    setError(null);
    try {
      const finished = await stopRecording();
      const endedAt = new Date();
      const begunAt = startedAt.current ?? endedAt;
      const id = sessionId.current;

      if (id !== null) {
        await finishSession({
          id,
          title: defaultTitle(begunAt),
          endedAt: endedAt.toISOString(),
          durationMs: finished.duration_ms,
          audioPath: finished.directory,
        });
        compressRecording(finished.directory).catch(() => undefined);
      }

      setLevels({ mic: 0, system: 0 });
      setElapsedMs(0);
      startedAt.current = null;
      sessionId.current = null;
      setPhase("idle");
      if (id !== null) onFinished(id);
    } catch (cause: unknown) {
      setError(String(cause));
      setPhase("idle");
    }
  }

  async function runImport() {
    setError(null);
    setLines([]);
    importStarted.current = Date.now();
    setImporting({ done: 0, total: 1, eta: 0 });
    try {
      const begunAt = new Date();
      const id = await createSession(begunAt.toISOString());
      sessionId.current = id;
      const imported = await importAudio(id);
      await finishSession({
        id,
        title: imported.file_name.replace(/\.[^.]+$/, ""),
        endedAt: new Date().toISOString(),
        durationMs: imported.duration_ms,
        audioPath: imported.directory,
      });
      sessionId.current = null;
      onFinished(id);
    } catch (cause: unknown) {
      const message = String(cause);
      if (!message.includes("Nessun file scelto")) setError(message);
      sessionId.current = null;
    } finally {
      setImporting(null);
    }
  }

  const waitingForSpeech =
    recording && lines.length === 0 && Date.now() - lastSpeechMs.current < 60000;

  return (
    <div className="relative flex h-full w-full flex-col">
      {/* Resta agganciata in alto mentre la trascrizione scorre: altrimenti
          l'avanzamento sparisce appena il testo cresce. */}
      {importing && (
        <div className="sticky top-0 z-10 border-b border-edge bg-surface-raised/95 px-6 py-2.5 backdrop-blur">
          <div className="mx-auto flex max-w-2xl items-center gap-3">
            <Spinner />
            <div className="flex-1">
              <div className="mb-1 flex items-baseline justify-between text-[11px]">
                <span>Trascrizione in corso</span>
                <span className="text-ink-muted">
                  {formatClock(importing.done)} di {formatClock(importing.total)}
                  {importing.eta > 0 && ` · ${formatClock(importing.eta)} rimanenti`}
                </span>
              </div>
              <div className="h-1 overflow-hidden rounded-full bg-surface-sunken">
                <div
                  className="h-full rounded-full bg-accent transition-[width] duration-300"
                  style={{
                    width: `${Math.round((importing.done / Math.max(importing.total, 1)) * 100)}%`,
                  }}
                />
              </div>
            </div>
            <span className="w-12 text-right text-lg font-light tabular-nums">
              {Math.round((importing.done / Math.max(importing.total, 1)) * 100)}%
            </span>
          </div>
        </div>
      )}

      <div className="brief-drag flex w-full flex-1 flex-col items-center gap-7 overflow-y-auto px-8 pb-10 pt-14">
      <div className="flex flex-col items-center gap-5">
        <div className="flex items-center gap-3">
          {recording && (
            <span className="brief-live-dot h-2.5 w-2.5 rounded-full bg-live" />
          )}
          <span className="text-6xl font-light tabular-nums tracking-tight">
            {formatClock(elapsedMs)}
          </span>
        </div>

        <button
          onClick={recording ? end : begin}
          disabled={busy}
          className={`px-7 py-2.5 text-[13px] disabled:opacity-50 ${
            recording
              ? "brief-button text-live"
              : "brief-button-primary"
          }`}
        >
          {phase === "preparing" ? (
            <Spinner label="Preparazione…" />
          ) : phase === "finishing" ? (
            <Spinner label="Chiusura…" />
          ) : recording ? (
            "Ferma registrazione"
          ) : (
            "Avvia registrazione"
          )}
        </button>

        <p className="text-xs text-ink-muted">
          {recording ? "Barra spaziatrice per fermare" : "Barra spaziatrice per avviare"}
        </p>

        {lines.length > 0 && !recording && !busy && (
          <span className="text-xs text-ink-muted">
            {lines.length} {lines.length === 1 ? "riga trascritta" : "righe trascritte"}
          </span>
        )}

        {!recording && !busy && (
          <button
            onClick={runImport}
            disabled={importing !== null}
            className="text-xs text-ink-muted underline underline-offset-4 hover:text-ink disabled:opacity-50"
          >
            oppure importa un file audio
          </button>
        )}
      </div>

      {modelReady === false && phase === "idle" && !download && (
        <p className="max-w-sm rounded-lg border border-edge bg-surface-raised px-4 py-3 text-center text-xs leading-relaxed text-ink-muted">
          Al primo avvio Brief scarica il modello di trascrizione (190 MB). Serve
          una connessione solo per questo.
        </p>
      )}

      <div className="w-full max-w-md space-y-2.5">
        <LevelMeter label="Microfono" rms={levels.mic} active={recording} />
        <LevelMeter label="Sistema" rms={levels.system} active={recording} />
      </div>

      {importing && (
        <div className="w-full max-w-md space-y-2.5 rounded-xl border border-edge bg-surface-raised px-4 py-3.5">
          <div className="flex items-baseline justify-between">
            <span className="flex items-center gap-2 text-xs">
              <Spinner />
              Trascrizione del file
            </span>
            <span className="text-2xl font-light tabular-nums">
              {Math.round((importing.done / Math.max(importing.total, 1)) * 100)}%
            </span>
          </div>

          <div className="h-1.5 overflow-hidden rounded-full bg-surface-sunken">
            <div
              className="h-full rounded-full bg-accent transition-[width] duration-300"
              style={{
                width: `${Math.round((importing.done / Math.max(importing.total, 1)) * 100)}%`,
              }}
            />
          </div>

          <div className="flex justify-between text-[11px] text-ink-muted">
            <span>
              {formatClock(importing.done)} di {formatClock(importing.total)}
            </span>
            {importing.eta > 0 && (
              <span>circa {formatClock(importing.eta)} rimanenti</span>
            )}
          </div>
        </div>
      )}

      {download && (
        <div className="w-full max-w-md space-y-2 rounded-lg border border-edge bg-surface-raised px-4 py-3">
          <div className="flex items-center justify-between">
            <span className="flex items-center gap-2 text-xs">
              <Spinner />
              {download.label}
            </span>
            <span className="font-mono text-xs text-ink-muted">
              {formatSize(download.downloaded)} / {formatSize(download.total)}
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-surface-sunken">
            <div
              className="h-full rounded-full bg-accent transition-[width]"
              style={{
                width: `${Math.round((download.downloaded / download.total) * 100)}%`,
              }}
            />
          </div>
        </div>
      )}

      {(lines.length > 0 || recording) && (
        <div className="w-full max-w-2xl flex-1 space-y-3 rounded-xl border border-edge bg-surface-sunken/60 p-5">
          {lines.map((line, index) => (
            <p key={index} className="brief-rise text-sm leading-relaxed">
              <span
                className={`mr-2 text-xs font-medium ${
                  line.track === "mic" ? "text-accent" : "text-ink-muted"
                }`}
              >
                {speakerOf(line.track)}
                {line.speaker !== null && line.track !== "mic" && ` ${line.speaker + 1}`}
              </span>
              {line.text}
            </p>
          ))}

          {recording && (
            <div className="space-y-2 pt-1">
              <div className="brief-skeleton h-3 w-3/4 rounded" />
              <div className="brief-skeleton h-3 w-1/2 rounded" />
              <p className="pt-1 text-xs text-ink-muted">
                {waitingForSpeech
                  ? "In ascolto…"
                  : "Trascrizione in corso, il testo appare alle pause"}
              </p>
            </div>
          )}
          <div ref={transcriptEnd} />
        </div>
      )}

      {!analysisReady && phase === "idle" && (
        <p className="max-w-sm rounded-lg border border-edge bg-surface-raised px-4 py-3 text-center text-xs leading-relaxed text-ink-muted">
          Per generare i report serve una chiave API: impostala dall'ingranaggio
          in alto a sinistra. La registrazione e la trascrizione funzionano
          comunque, e restano sul tuo Mac.
        </p>
      )}

      {systemWarning && (
        <p className="max-w-md rounded-lg border border-edge bg-surface-raised px-4 py-3 text-xs leading-relaxed text-ink-muted">
          {systemWarning}
        </p>
      )}

      {error && (
        <p className="max-w-md rounded-lg border border-live/40 bg-live/10 px-4 py-3 text-xs leading-relaxed text-live">
          {error}
        </p>
      )}
      </div>
    </div>
  );
}
