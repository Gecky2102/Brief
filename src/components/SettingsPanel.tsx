import { useCallback, useEffect, useState } from "react";
import Spinner from "./Spinner";
import {
  deleteModel,
  getSettings,
  hasApiKey,
  setApiKey,
  setSettings,
  storageReport,
  type Provider,
  type Quality,
  type Settings,
  type StorageReport,
} from "../lib/recorder";

type Props = {
  onClose: () => void;
};

const PROVIDERS: { value: Provider; label: string; hint: string }[] = [
  {
    value: "anthropic",
    label: "Anthropic",
    hint: "console.anthropic.com › API keys",
  },
  { value: "openai", label: "OpenAI", hint: "platform.openai.com › API keys" },
  {
    value: "google",
    label: "Google",
    hint: "aistudio.google.com › Get API key",
  },
  {
    value: "openrouter",
    label: "OpenRouter",
    hint: "openrouter.ai › Keys — un'unica chiave per molti modelli",
  },
  {
    value: "compatible",
    label: "Compatibile OpenAI",
    hint: "Qualsiasi servizio con API in stile OpenAI: indica l'indirizzo",
  },
];

const DEFAULT_MODELS: Record<Provider, string> = {
  anthropic: "claude-sonnet-5",
  openai: "gpt-5",
  google: "gemini-2.5-flash",
  openrouter: "anthropic/claude-sonnet-5",
  compatible: "",
};

const QUALITIES: { value: Quality; label: string; detail: string }[] = [
  {
    value: "fast",
    label: "Veloce",
    detail: "Modello da 190 MB. Trascrive in tempo reale, va bene per una voce sola.",
  },
  {
    value: "accurate",
    label: "Accurata",
    detail:
      "Modello da 574 MB. Regge molto meglio parlato spontaneo, dialetti e più persone.",
  },
];

function formatBytes(bytes: number): string {
  if (bytes >= 1073741824) return `${(bytes / 1073741824).toFixed(1)} GB`;
  return `${Math.round(bytes / 1048576)} MB`;
}

export default function SettingsPanel({ onClose }: Props) {
  const [settings, setLocalSettings] = useState<Settings | null>(null);
  const [keyPresent, setKeyPresent] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [storage, setStorage] = useState<StorageReport | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshStorage = useCallback(() => {
    storageReport().then(setStorage).catch(() => undefined);
  }, []);

  useEffect(() => {
    getSettings().then(setLocalSettings).catch(() => undefined);
    hasApiKey().then(setKeyPresent).catch(() => undefined);
    refreshStorage();
  }, [refreshStorage]);

  async function save(next: Settings) {
    setLocalSettings(next);
    setError(null);
    try {
      await setSettings(next);
      refreshStorage();
    } catch (cause: unknown) {
      setError(String(cause));
    }
  }

  async function saveKey() {
    setError(null);
    try {
      await setApiKey(keyDraft);
      setKeyDraft("");
      setKeyPresent(await hasApiKey());
      setNotice("Chiave salvata nel portachiavi di sistema.");
    } catch (cause: unknown) {
      setError(String(cause));
    }
  }

  async function removeModel(fileName: string) {
    setError(null);
    try {
      await deleteModel(fileName);
      refreshStorage();
    } catch (cause: unknown) {
      setError(String(cause));
    }
  }

  if (!settings) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner size="md" />
      </div>
    );
  }

  const provider = PROVIDERS.find((p) => p.value === settings.provider);

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <header className="flex items-center justify-between border-b border-edge px-8 py-5">
        <h2 className="text-lg font-semibold">Impostazioni</h2>
        <button
          onClick={onClose}
          className="rounded-md border border-edge px-3 py-1.5 text-xs hover:bg-surface-raised"
        >
          Chiudi
        </button>
      </header>

      <div className="space-y-10 px-8 py-6">
        <section className="space-y-3">
          <div>
            <h3 className="text-sm font-medium">Trascrizione</h3>
            <p className="text-xs text-ink-muted">
              Avviene sul tuo Mac. L'audio non esce mai dal computer.
            </p>
          </div>
          <div className="flex gap-2">
            {QUALITIES.map((option) => (
              <button
                key={option.value}
                onClick={() => save({ ...settings, quality: option.value })}
                className={`flex-1 rounded-lg border px-3 py-2.5 text-left transition-colors ${
                  settings.quality === option.value
                    ? "border-accent bg-accent-soft"
                    : "border-edge hover:bg-surface-raised"
                }`}
              >
                <span className="block text-xs font-medium">{option.label}</span>
                <span className="mt-1 block text-[11px] leading-snug text-ink-muted">
                  {option.detail}
                </span>
              </button>
            ))}
          </div>
        </section>

        <section className="space-y-3">
          <div>
            <h3 className="text-sm font-medium">Riassunto</h3>
            <p className="text-xs leading-relaxed text-ink-muted">
              Il testo della trascrizione viene inviato al servizio scelto. Questa
              è l'unica parte di Brief che esce dal tuo Mac.
            </p>
          </div>

          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {PROVIDERS.map((option) => (
              <button
                key={option.value}
                onClick={() =>
                  save({
                    ...settings,
                    provider: option.value,
                    model: DEFAULT_MODELS[option.value],
                  })
                }
                className={`rounded-lg border px-3 py-2 text-xs transition-colors ${
                  settings.provider === option.value
                    ? "border-accent bg-accent-soft"
                    : "border-edge hover:bg-surface-raised"
                }`}
              >
                {option.label}
              </button>
            ))}
          </div>

          {provider && (
            <p className="text-[11px] text-ink-muted">{provider.hint}</p>
          )}

          <label className="block space-y-1.5">
            <span className="text-xs text-ink-muted">Modello</span>
            <input
              value={settings.model}
              onChange={(event) =>
                setLocalSettings({ ...settings, model: event.target.value })
              }
              onBlur={() => save(settings)}
              placeholder="nome del modello"
              className="w-full rounded-md border border-edge bg-surface-raised px-3 py-2 text-sm outline-none focus:border-accent"
            />
          </label>

          {settings.provider === "compatible" && (
            <label className="block space-y-1.5">
              <span className="text-xs text-ink-muted">
                Indirizzo del servizio
              </span>
              <input
                value={settings.base_url}
                onChange={(event) =>
                  setLocalSettings({ ...settings, base_url: event.target.value })
                }
                onBlur={() => save(settings)}
                placeholder="https://esempio.com/v1"
                className="w-full rounded-md border border-edge bg-surface-raised px-3 py-2 text-sm outline-none focus:border-accent"
              />
            </label>
          )}

          <label className="block space-y-1.5">
            <span className="text-xs text-ink-muted">
              Chiave API{" "}
              {keyPresent && <span className="text-accent">— configurata</span>}
            </span>
            <div className="flex gap-2">
              <input
                type="password"
                value={keyDraft}
                onChange={(event) => setKeyDraft(event.target.value)}
                placeholder={keyPresent ? "••••••••  (sostituisci)" : "incolla qui la chiave"}
                className="flex-1 rounded-md border border-edge bg-surface-raised px-3 py-2 text-sm outline-none focus:border-accent"
              />
              <button
                onClick={saveKey}
                disabled={!keyDraft.trim()}
                className="rounded-md bg-accent px-3 py-2 text-xs font-medium text-white disabled:opacity-40"
              >
                Salva
              </button>
            </div>
            <span className="block text-[11px] leading-snug text-ink-muted">
              Custodita nel portachiavi di macOS, non in un file di
              configurazione.
            </span>
          </label>
        </section>

        <section className="space-y-3">
          <div className="flex items-baseline justify-between">
            <h3 className="text-sm font-medium">Spazio su disco</h3>
            {storage && (
              <span className="text-[11px] text-ink-muted">
                {formatBytes(storage.used_bytes)} usati ·{" "}
                {formatBytes(storage.free_bytes)} liberi
              </span>
            )}
          </div>

          <div className="space-y-2">
            {storage?.models.map((model) => (
              <div
                key={model.file_name}
                className="flex items-center gap-3 rounded-lg border border-edge px-3 py-2.5"
              >
                <div className="min-w-0 flex-1">
                  <span className="block text-xs font-medium">
                    {model.label}
                    {model.in_use && (
                      <span className="ml-2 text-[10px] text-accent">in uso</span>
                    )}
                  </span>
                  <span className="block text-[11px] text-ink-muted">
                    {model.on_disk === 0
                      ? `non scaricato · ${formatBytes(model.bytes)}`
                      : model.complete
                        ? formatBytes(model.on_disk)
                        : `scaricato a metà · ${formatBytes(model.on_disk)} di ${formatBytes(model.bytes)}`}
                  </span>
                </div>
                {model.on_disk > 0 && (
                  <button
                    onClick={() => removeModel(model.file_name)}
                    className="shrink-0 rounded-md border border-edge px-2.5 py-1 text-[11px] text-ink-muted hover:border-live/50 hover:text-live"
                  >
                    Elimina
                  </button>
                )}
              </div>
            ))}
          </div>
          <p className="text-[11px] leading-snug text-ink-muted">
            Un modello eliminato viene riscaricato quando serve. I download
            interrotti riprendono da dove erano rimasti.
          </p>
        </section>

        {notice && <p className="text-xs text-ink-muted">{notice}</p>}
        {error && (
          <p className="rounded-md border border-live/40 bg-live/10 px-3 py-2 text-xs text-live">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
