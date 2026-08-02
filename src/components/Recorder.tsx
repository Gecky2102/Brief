import { useEffect, useRef, useState } from "react";
import LevelMeter from "./LevelMeter";
import Spinner from "./Spinner";
import { addSegment, createSession, finishSession } from "../lib/db";
import { speakerOf } from "../lib/markdown";
import {
  compressRecording,
  modelsStatus,
  onDownloadProgress,
  onLevel,
  onSegment,
  onTranscriptError,
  startRecording,
  stopRecording,
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
  const [error, setError] = useState<string | null>(null);

  const startedAt = useRef<Date | null>(null);
  const sessionId = useRef<number | null>(null);
  const transcriptEnd = useRef<HTMLDivElement | null>(null);
  const lastSpeechMs = useRef<number>(0);

  const recording = phase === "recording";
  const busy = phase === "preparing" || phase === "finishing";

  useEffect(() => {
    modelsStatus()
      .then((status) => setModelReady(status.transcription))
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
        }).catch(() => undefined);
      }),
      onTranscriptError(setError),
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

  const waitingForSpeech =
    recording && lines.length === 0 && Date.now() - lastSpeechMs.current < 60000;

  return (
    <div className="flex h-full w-full flex-col items-center gap-7 overflow-y-auto px-8 py-10">
      <div className="flex flex-col items-center gap-5">
        <div className="flex items-center gap-3">
          {recording && (
            <span className="brief-live-dot h-2.5 w-2.5 rounded-full bg-live" />
          )}
          <span className="font-mono text-6xl tabular-nums tracking-tight">
            {formatClock(elapsedMs)}
          </span>
        </div>

        <button
          onClick={recording ? end : begin}
          disabled={busy}
          className={`rounded-full px-7 py-3 text-sm font-medium transition-all disabled:opacity-60 ${
            recording
              ? "border border-edge bg-surface-raised text-ink hover:border-live hover:text-live"
              : "bg-accent text-white hover:brightness-110"
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

      {error && (
        <p className="max-w-md rounded-lg border border-live/40 bg-live/10 px-4 py-3 text-xs leading-relaxed text-live">
          {error}
        </p>
      )}
    </div>
  );
}
