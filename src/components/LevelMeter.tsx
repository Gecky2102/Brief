type Props = {
  label: string;
  rms: number;
  active: boolean;
};

/// L'RMS grezzo occupa la parte bassa della scala: una curva di potenza lo
/// rende leggibile senza far sbattere l'indicatore a fondo scala.
function toWidth(rms: number): number {
  return Math.min(100, Math.round(Math.pow(Math.min(rms, 1), 0.45) * 130));
}

export default function LevelMeter({ label, rms, active }: Props) {
  return (
    <div className="flex items-center gap-3">
      <span className="w-24 shrink-0 text-xs text-ink-muted">{label}</span>
      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-raised">
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-75 ease-out"
          style={{ width: `${active ? toWidth(rms) : 0}%` }}
        />
      </div>
    </div>
  );
}
