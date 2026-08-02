import { useEffect, useRef, useState } from "react";
import LevelMeter from "./LevelMeter";
import { createSession } from "../lib/db";
import {
  onLevel,
  startRecording,
  stopRecording,
  type FinishedRecording,
} from "../lib/recorder";

type Props = {
  onFinished: () => void;
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

export default function Recorder({ onFinished }: Props) {
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);
  const [levels, setLevels] = useState({ mic: 0, system: 0 });
  const [elapsedMs, setElapsedMs] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const startedAt = useRef<Date | null>(null);

  useEffect(() => {
    const subscription = onLevel((event) => {
      setLevels((current) => ({ ...current, [event.track]: event.rms }));
      setElapsedMs(event.elapsed_ms);
    });
    return () => {
      subscription.then((unlisten) => unlisten());
    };
  }, []);

  // Gli eventi di livello arrivano solo quando c'è segnale: senza un tick
  // dedicato il cronometro resterebbe fermo durante i silenzi.
  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => {
      if (startedAt.current) {
        setElapsedMs(Date.now() - startedAt.current.getTime());
      }
    }, 500);
    return () => window.clearInterval(timer);
  }, [recording]);

  async function toggle() {
    setBusy(true);
    setError(null);
    try {
      if (recording) {
        const finished: FinishedRecording = await stopRecording();
        const endedAt = new Date();
        const begin = startedAt.current ?? endedAt;
        await createSession({
          title: defaultTitle(begin),
          startedAt: begin.toISOString(),
          endedAt: endedAt.toISOString(),
          durationMs: finished.duration_ms,
          audioPath: finished.directory,
        });
        setRecording(false);
        setLevels({ mic: 0, system: 0 });
        setElapsedMs(0);
        startedAt.current = null;
        onFinished();
      } else {
        await startRecording();
        startedAt.current = new Date();
        setElapsedMs(0);
        setRecording(true);
      }
    } catch (cause: unknown) {
      setError(String(cause));
      setRecording(false);
      startedAt.current = null;
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="w-full max-w-md space-y-6">
      <div className="flex flex-col items-center gap-4">
        <span className="font-mono text-4xl tabular-nums">
          {formatClock(elapsedMs)}
        </span>
        <button
          onClick={toggle}
          disabled={busy}
          className="rounded-full bg-accent px-6 py-2.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-50"
        >
          {recording ? "Ferma registrazione" : "Avvia registrazione"}
        </button>
      </div>

      <div className="space-y-2">
        <LevelMeter label="Microfono" rms={levels.mic} active={recording} />
        <LevelMeter label="Sistema" rms={levels.system} active={recording} />
      </div>

      {error && (
        <p className="rounded-md border border-edge bg-surface-raised px-3 py-2 text-xs leading-relaxed text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
