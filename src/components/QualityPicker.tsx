import { useEffect, useState } from "react";
import { getSettings, setSettings, type Quality } from "../lib/recorder";

const OPTIONS: {
  value: Quality;
  label: string;
  detail: string;
}[] = [
  {
    value: "fast",
    label: "Veloce",
    detail: "Modelli leggeri, 2,3 GB in tutto. Trascrive in tempo reale.",
  },
  {
    value: "accurate",
    label: "Accurata",
    detail:
      "Modelli grandi, 5,3 GB. Regge meglio parlato spontaneo e dialetti, e i riassunti sono più concreti.",
  },
];

export default function QualityPicker() {
  const [quality, setQuality] = useState<Quality | null>(null);

  useEffect(() => {
    getSettings()
      .then((settings) => setQuality(settings.quality))
      .catch(() => setQuality("fast"));
  }, []);

  async function choose(value: Quality) {
    setQuality(value);
    await setSettings({ quality: value }).catch(() => undefined);
  }

  if (quality === null) return null;

  return (
    <div className="w-full max-w-md space-y-2">
      <span className="text-xs text-ink-muted">Qualità</span>
      <div className="flex gap-2">
        {OPTIONS.map((option) => (
          <button
            key={option.value}
            onClick={() => choose(option.value)}
            className={`flex-1 rounded-lg border px-3 py-2 text-left transition-colors ${
              quality === option.value
                ? "border-accent bg-accent-soft"
                : "border-edge hover:bg-surface-raised"
            }`}
          >
            <span className="block text-xs font-medium">{option.label}</span>
            <span className="mt-0.5 block text-[11px] leading-snug text-ink-muted">
              {option.detail}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
