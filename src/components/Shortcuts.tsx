type Props = {
  onClose: () => void;
};

export default function Shortcuts({ onClose }: Props) {
  const isMac =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Mac");
  const mod = isMac ? "⌘" : "Ctrl+";
  const shiftMod = isMac ? "⌘⇧" : "Ctrl+Shift+";

  const scorciatoie: [string, string][] = [
    ["Spazio", "Avvia o ferma la registrazione"],
    [`${mod}N`, "Nuova sessione"],
    [`${mod}F`, "Cerca nelle trascrizioni"],
    [`${mod}R`, "Genera o rigenera il report"],
    [`${mod}P`, "Esporta il report in PDF (o stampa)"],
    [`${shiftMod}C`, "Copia la sessione negli appunti"],
    [`${mod},`, "Impostazioni"],
    ["Esc", "Svuota la ricerca o torna indietro"],
    ["?", "Mostra questo riquadro"],
  ];
  return (
    <div
      onClick={onClose}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-8"
    >
      <div
        onClick={(event) => event.stopPropagation()}
        className="w-full max-w-sm rounded-2xl border border-edge bg-surface-raised p-5 shadow-2xl"
      >
        <div className="mb-3 flex items-baseline justify-between">
          <h3 className="text-sm font-semibold">Scorciatoie</h3>
          <button
            onClick={onClose}
            className="text-xs text-ink-muted hover:text-ink"
          >
            Chiudi
          </button>
        </div>

        <dl className="space-y-1.5">
          {scorciatoie.map(([tasto, azione]) => (
            <div key={tasto} className="flex items-baseline gap-3">
              <dt className="w-16 shrink-0 text-right font-mono text-[11px] text-ink-muted">
                {tasto}
              </dt>
              <dd className="text-xs">{azione}</dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  );
}
