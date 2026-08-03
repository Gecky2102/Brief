import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";

type Props = {
  path: string;
  /// Istante a cui saltare, in millisecondi. Cambia quando si clicca un
  /// segmento della trascrizione.
  seekTo: number | null;
  onTime: (ms: number) => void;
};

function formatClock(seconds: number): string {
  const totale = Math.floor(seconds);
  return `${String(Math.floor(totale / 60)).padStart(2, "0")}:${String(totale % 60).padStart(2, "0")}`;
}

export default function AudioPlayer({ path, seekTo, onTime }: Props) {
  const audio = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [speed, setSpeed] = useState(1);

  useEffect(() => {
    if (seekTo === null || !audio.current) return;
    audio.current.currentTime = seekTo / 1000;
    void audio.current.play();
    setPlaying(true);
  }, [seekTo]);

  // Frecce per spostarsi di dieci secondi: riascoltare una parola dubbia
  // senza cercare col cursore.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
      if (!audio.current) return;

      if (event.key === "ArrowLeft") {
        audio.current.currentTime = Math.max(audio.current.currentTime - 10, 0);
      } else if (event.key === "ArrowRight") {
        audio.current.currentTime += 10;
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  function toggle() {
    if (!audio.current) return;
    if (playing) {
      audio.current.pause();
    } else {
      void audio.current.play();
    }
    setPlaying(!playing);
  }

  return (
    <div className="flex items-center gap-3 rounded-xl border border-edge bg-surface-raised/60 px-3 py-2">
      <audio
        ref={audio}
        src={convertFileSrc(path)}
        preload="metadata"
        onTimeUpdate={(event) => {
          const secondi = event.currentTarget.currentTime;
          setPosition(secondi);
          onTime(secondi * 1000);
        }}
        onLoadedMetadata={(event) => setDuration(event.currentTarget.duration)}
        onEnded={() => setPlaying(false)}
      />

      <button
        onClick={toggle}
        className="brief-button flex h-7 w-7 shrink-0 items-center justify-center"
        title={playing ? "Pausa" : "Riproduci"}
      >
        {playing ? "❚❚" : "▶"}
      </button>

      <input
        type="range"
        min={0}
        max={Math.max(duration, 0.1)}
        step={0.1}
        value={position}
        onChange={(event) => {
          const secondi = Number(event.target.value);
          if (audio.current) audio.current.currentTime = secondi;
          setPosition(secondi);
        }}
        className="h-1 flex-1 cursor-pointer accent-[var(--accent)]"
      />

      <span className="shrink-0 font-mono text-[11px] tabular-nums text-ink-muted">
        {formatClock(position)} / {formatClock(duration)}
      </span>

      <button
        onClick={() => {
          const prossima = speed === 1 ? 1.5 : speed === 1.5 ? 2 : 1;
          setSpeed(prossima);
          if (audio.current) audio.current.playbackRate = prossima;
        }}
        title="Velocità di riproduzione"
        className="brief-button shrink-0 px-2 py-0.5 font-mono text-[11px]"
      >
        {speed}×
      </button>
    </div>
  );
}
