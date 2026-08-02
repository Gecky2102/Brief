import { useEffect, useRef, useState } from "react";
import LevelMeter from "./LevelMeter";
import { addSegment, createSession, finishSession } from "../lib/db";
import { speakerOf } from "../lib/markdown";
import {
  compressRecording,
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
  return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
}

export default function Recorder({ onFinished }: Props) {
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);
  const [levels, setLevels] = useState({ mic: 0, system: 0 });
  const [elapsedMs, setElapsedMs] = useState(0);
  const [lines, setLines] = useState<SegmentEvent[]>([]);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const startedAt = useRef<Date | null>(null);
  const sessionId = useRef<number | null>(null);
  const transcriptEnd = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const subscriptions = [
      onLevel((event) => {
        setLevels((current) => ({ ...current, [event.track]: event.rms }));
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

  async function begin() {
    setBusy(true);
    setError(null);
    setLines([]);
    try {
      const begunAt = new Date();
      const id = await createSession(begunAt.toISOString());
      sessionId.current = id;
      await startRecording(id);
      startedAt.current = begunAt;
      setElapsedMs(0);
      setRecording(true);
    } catch (cause: unknown) {
      setError(String(cause));
      sessionId.current = null;
      startedAt.current = null;
    } finally {
      setBusy(false);
      setDownload(null);
    }
  }

  async function end() {
    setBusy(true);
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
        // La compressione può richiedere qualche secondo su registrazioni
        // lunghe: non deve trattenere l'interfaccia.
        compressRecording(finished.directory).catch(() => undefined);
      }

      setRecording(false);
      setLevels({ mic: 0, system: 0 });
      setElapsedMs(0);
      startedAt.current = null;
      sessionId.current = null;
      if (id !== null) onFinished(id);
    } catch (cause: unknown) {
      setError(String(cause));
      setRecording(false);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-6 p-8">
      <div className="flex flex-col items-center gap-4">
        <span className="font-mono text-5xl tabular-nums">
          {formatClock(elapsedMs)}
        </span>
        <button
          onClick={recording ? end : begin}
          disabled={busy}
          className={`rounded-full px-6 py-2.5 text-sm font-medium transition-opacity hover:opacity-90 disabled:opacity-50 ${
            recording
              ? "border border-edge bg-surface-raised text-ink"
              : "bg-accent text-white"
          }`}
        >
          {busy
            ? "Un momento…"
            : recording
              ? "Ferma registrazione"
              : "Avvia registrazione"}
        </button>
      </div>

      <div className="w-full max-w-md space-y-2">
        <LevelMeter label="Microfono" rms={levels.mic} active={recording} />
        <LevelMeter label="Sistema" rms={levels.system} active={recording} />
      </div>

      {download && (
        <div className="w-full max-w-md space-y-2">
          <p className="text-xs text-ink-muted">
            {download.label}: {formatSize(download.downloaded)} di{" "}
            {formatSize(download.total)}
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

      {lines.length > 0 && (
        <div className="w-full max-w-2xl flex-1 space-y-3 overflow-y-auto rounded-lg border border-edge p-4">
          {lines.map((line, index) => (
            <p key={index} className="text-sm leading-relaxed">
              <span className="mr-2 text-xs font-medium text-accent">
                {speakerOf(line.track)}
              </span>
              {line.text}
            </p>
          ))}
          <div ref={transcriptEnd} />
        </div>
      )}

      {error && (
        <p className="max-w-md rounded-md border border-edge bg-surface-raised px-3 py-2 text-xs leading-relaxed text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
