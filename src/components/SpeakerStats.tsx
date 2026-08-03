import type { Segment, Speaker } from "../lib/db";
import { speakerColor } from "./SpeakerBar";

type Props = {
  segments: Segment[];
  speakers: Speaker[];
};

/// Quanto ha parlato ciascuno: dà subito il polso di com'è andata la riunione,
/// e rende evidente quando una voce è stata divisa in due per sbaglio.
export default function SpeakerStats({ segments, speakers }: Props) {
  const durate = new Map<string, { ms: number; colore: string }>();

  for (const segment of segments) {
    const durata = Math.max(segment.end_ms - segment.start_ms, 0);
    const nome =
      segment.track === "mic"
        ? "Io"
        : segment.speaker_label || "Non attribuito";
    const colore =
      segment.track === "mic"
        ? "var(--accent)"
        : segment.speaker_id !== null
          ? speakerColor(
              speakers.find((v) => v.id === segment.speaker_id)?.cluster_index ??
                0,
            )
          : "var(--ink-muted)";

    const attuale = durate.get(nome) ?? { ms: 0, colore };
    durate.set(nome, { ms: attuale.ms + durata, colore });
  }

  const voci = [...durate.entries()].sort((a, b) => b[1].ms - a[1].ms);
  const totale = voci.reduce((somma, [, v]) => somma + v.ms, 0);
  if (totale === 0 || voci.length < 2) return null;

  return (
    <div className="space-y-2 rounded-xl border border-edge bg-surface-raised/60 p-3">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-muted">
        Distribuzione degli interventi
      </span>

      <div className="flex h-2 overflow-hidden rounded-full">
        {voci.map(([nome, dati]) => (
          <div
            key={nome}
            style={{
              width: `${(dati.ms / totale) * 100}%`,
              backgroundColor: dati.colore,
            }}
            title={`${nome}: ${Math.round((dati.ms / totale) * 100)}%`}
          />
        ))}
      </div>

      <div className="flex flex-wrap gap-x-4 gap-y-1 text-[11px]">
        {voci.map(([nome, dati]) => (
          <span key={nome} className="flex items-center gap-1.5">
            <span
              className="h-2 w-2 rounded-full"
              style={{ backgroundColor: dati.colore }}
            />
            <span>{nome}</span>
            <span className="text-ink-muted">
              {Math.round((dati.ms / totale) * 100)}% ·{" "}
              {Math.round(dati.ms / 60000)} min
            </span>
          </span>
        ))}
      </div>
    </div>
  );
}
