import { useState } from "react";
import type { Speaker } from "../lib/db";

type Props = {
  speakers: Speaker[];
  counts: Record<number, number>;
  onRename: (id: number, label: string) => void;
  onMerge: (from: number, into: number) => void;
};

/// Colori stabili per voce: lo stesso indice dà sempre la stessa tinta, così
/// scorrendo la trascrizione si riconosce chi parla a colpo d'occhio.
export const SPEAKER_COLORS = [
  "#0a84ff",
  "#30d158",
  "#ff9f0a",
  "#bf5af2",
  "#ff375f",
  "#64d2ff",
  "#ffd60a",
  "#ac8e68",
];

export function speakerColor(index: number): string {
  return SPEAKER_COLORS[index % SPEAKER_COLORS.length];
}

export default function SpeakerBar({
  speakers,
  counts,
  onRename,
  onMerge,
}: Props) {
  const [editing, setEditing] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const [merging, setMerging] = useState<number | null>(null);

  if (speakers.length === 0) return null;

  return (
    <div className="space-y-2 rounded-xl border border-edge bg-surface-raised/60 p-3">
      <div className="flex items-baseline justify-between">
        <span className="text-[11px] font-medium uppercase tracking-wide text-ink-muted">
          Voci riconosciute
        </span>
        {merging !== null && (
          <span className="text-[11px] text-ink-muted">
            Scegli con quale unirla, oppure{" "}
            <button
              onClick={() => setMerging(null)}
              className="underline underline-offset-2"
            >
              annulla
            </button>
          </span>
        )}
      </div>

      <div className="flex flex-wrap gap-1.5">
        {speakers.map((speaker) => {
          const colore = speakerColor(speaker.cluster_index);
          const interventi = counts[speaker.id] ?? 0;

          if (editing === speaker.id) {
            return (
              <input
                key={speaker.id}
                autoFocus
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onBlur={() => {
                  if (draft.trim()) onRename(speaker.id, draft.trim());
                  setEditing(null);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.currentTarget.blur();
                  if (event.key === "Escape") setEditing(null);
                }}
                className="brief-field w-32 px-2 py-1 text-xs"
              />
            );
          }

          return (
            <button
              key={speaker.id}
              onClick={() => {
                if (merging === null) {
                  setEditing(speaker.id);
                  setDraft(speaker.label);
                } else if (merging !== speaker.id) {
                  onMerge(merging, speaker.id);
                  setMerging(null);
                }
              }}
              onContextMenu={(event) => {
                event.preventDefault();
                setMerging(speaker.id);
              }}
              title={
                merging === null
                  ? "Clicca per rinominare, tasto destro per unirla a un'altra"
                  : "Clicca per unire qui"
              }
              className={`flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition-colors ${
                merging === speaker.id
                  ? "border-accent bg-accent-soft"
                  : "border-edge hover:bg-surface-sunken"
              }`}
            >
              <span
                className="h-2 w-2 shrink-0 rounded-full"
                style={{ backgroundColor: colore }}
              />
              <span className="font-medium">{speaker.label}</span>
              <span className="text-ink-muted">{interventi}</span>
            </button>
          );
        })}
      </div>

      <p className="text-[11px] leading-snug text-ink-muted">
        Clicca una voce per darle un nome. Tasto destro per unirla a un'altra,
        quando la stessa persona è stata divisa in due.
      </p>
    </div>
  );
}
