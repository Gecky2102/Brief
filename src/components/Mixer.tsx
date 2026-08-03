import { useEffect, useState } from "react";
import { setTrackGain, type Track } from "../lib/recorder";

type Props = {
  levels: { mic: number; system: number };
  active: boolean;
};

/// L'RMS grezzo occupa la parte bassa della scala: una curva di potenza lo
/// rende leggibile senza far sbattere l'indicatore a fondo scala.
function toPercent(rms: number): number {
  return Math.min(100, Math.round(Math.pow(Math.min(rms, 1), 0.45) * 130));
}

function Channel({
  label,
  track,
  rms,
  active,
}: {
  label: string;
  track: Track;
  rms: number;
  active: boolean;
}) {
  const [gain, setGain] = useState(1);
  const livello = active ? toPercent(rms) : 0;

  useEffect(() => {
    setTrackGain(track, gain).catch(() => undefined);
  }, [track, gain]);

  // Sopra il novanta per cento si è vicini alla saturazione, che peggiora la
  // trascrizione invece di migliorarla.
  const saturo = livello > 90;

  return (
    <div className="space-y-1.5">
      <div className="flex items-baseline justify-between text-[11px]">
        <span>{label}</span>
        <span className={saturo ? "text-live" : "text-ink-muted"}>
          {active ? (saturo ? "troppo alto" : `${livello}`) : "—"}
        </span>
      </div>

      <div className="flex h-1.5 gap-px overflow-hidden rounded-full bg-surface-sunken">
        <div
          className={`h-full transition-[width] duration-75 ${
            saturo ? "bg-live" : "bg-accent"
          }`}
          style={{ width: `${livello}%` }}
        />
      </div>

      <div className="flex items-center gap-2">
        <input
          type="range"
          min={0}
          max={4}
          step={0.1}
          value={gain}
          onChange={(event) => setGain(Number(event.target.value))}
          className="h-1 flex-1 cursor-pointer accent-[var(--accent)]"
        />
        <button
          onClick={() => setGain(1)}
          title="Riporta a zero"
          className="w-12 shrink-0 text-right font-mono text-[10px] text-ink-muted hover:text-ink"
        >
          {gain === 1 ? "0 dB" : `${gain.toFixed(1)}×`}
        </button>
      </div>
    </div>
  );
}

export default function Mixer({ levels, active }: Props) {
  return (
    <div className="w-full max-w-md space-y-3 rounded-xl border border-edge bg-surface-raised/50 p-3">
      <Channel label="Microfono" track="mic" rms={levels.mic} active={active} />
      <Channel
        label="Sistema"
        track="system"
        rms={levels.system}
        active={active}
      />
      <p className="text-[11px] leading-snug text-ink-muted">
        Alza il guadagno se il livello resta basso: la trascrizione peggiora
        molto sul parlato lontano dal microfono. Se diventa rosso, abbassalo.
      </p>
    </div>
  );
}
