import { useCallback, useEffect, useState } from "react";
import Spinner from "./Spinner";
import { MODELS, REPORT_LENGTHS, REPORT_STYLES } from "../lib/catalog";
import {
  deleteModel,
  getSettings,
  hasApiKey,
  setApiKey,
  setSettings,
  revealDataFolder,
  storageReport,
  testProvider,
  verifyModel,
  type Provider,
  type Quality,
  type ReportLength,
  type VoiceSensitivity,
  type ReportStyle,
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
  const [testing, setTesting] = useState(false);
  const [checking, setChecking] = useState<string | null>(null);
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

  async function provaConnessione() {
    setTesting(true);
    setError(null);
    setNotice(null);
    try {
      setNotice(await testProvider());
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setTesting(false);
    }
  }

  async function verifica(fileName: string, label: string) {
    setChecking(fileName);
    setError(null);
    setNotice(null);
    try {
      const integro = await verifyModel(fileName);
      if (integro) {
        setNotice(`${label}: file integro.`);
      } else {
        setError(
          `${label}: il file non corrisponde all'originale. Eliminalo e verrà riscaricato.`,
        );
      }
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setChecking(null);
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
      <header className="brief-drag flex items-center justify-between border-b border-edge px-8 pb-4 pt-12">
        <h2 className="text-[19px] font-semibold tracking-tight">Impostazioni</h2>
        <button
          onClick={onClose}
          className="brief-button px-3 py-1.5 text-xs"
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
            <h3 className="text-sm font-medium">Riconoscimento delle voci</h3>
            <p className="text-xs leading-relaxed text-ink-muted">
              Brief distingue chi parla confrontando l'impronta vocale. Se
              spezza la stessa persona in due voci abbassa la sensibilità; se
              confonde persone diverse, alzala.
            </p>
          </div>

          <div className="flex gap-2">
            {(
              [
                { value: "low", label: "Bassa", detail: "Meno voci distinte" },
                { value: "medium", label: "Media", detail: "Consigliata" },
                { value: "high", label: "Alta", detail: "Più voci distinte" },
              ] as const
            ).map((option) => (
              <button
                key={option.value}
                onClick={() =>
                  save({
                    ...settings,
                    voice_sensitivity: option.value as VoiceSensitivity,
                  })
                }
                className={`flex-1 rounded-lg border px-3 py-2 text-center transition-colors ${
                  settings.voice_sensitivity === option.value
                    ? "border-accent bg-accent-soft"
                    : "border-edge hover:bg-surface-raised"
                }`}
              >
                <span className="block text-xs font-medium">{option.label}</span>
                <span className="block text-[11px] text-ink-muted">
                  {option.detail}
                </span>
              </button>
            ))}
          </div>

          <label className="flex items-center gap-3">
            <span className="text-xs text-ink-muted">
              Persone attese (0 = decide Brief)
            </span>
            <input
              type="number"
              min={0}
              max={8}
              value={settings.expected_speakers}
              onChange={(event) =>
                setLocalSettings({
                  ...settings,
                  expected_speakers: Number(event.target.value),
                })
              }
              onBlur={() => save(settings)}
              className="brief-field w-16 px-2 py-1 text-center text-[13px]"
            />
          </label>
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

          <div className="space-y-1.5">
            <span className="text-xs text-ink-muted">Modello</span>
            {MODELS[settings.provider].length > 0 && (
              <div className="space-y-1">
                {MODELS[settings.provider].map((model) => (
                  <button
                    key={model.id}
                    onClick={() => save({ ...settings, model: model.id })}
                    className={`flex w-full items-baseline gap-2 rounded-lg border px-3 py-1.5 text-left transition-colors ${
                      settings.model === model.id
                        ? "border-accent bg-accent-soft"
                        : "border-edge hover:bg-surface-raised"
                    }`}
                  >
                    <span className="text-xs font-medium">{model.id}</span>
                    <span className="text-[11px] text-ink-muted">
                      {model.note}
                    </span>
                  </button>
                ))}
              </div>
            )}
            <input
              value={settings.model}
              onChange={(event) =>
                setLocalSettings({ ...settings, model: event.target.value })
              }
              onBlur={() => save(settings)}
              placeholder="oppure scrivi il nome di un altro modello"
              className="brief-field w-full px-3 py-1.5 text-[13px]"
            />
          </div>

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
                className="brief-field w-full px-3 py-1.5 text-[13px]"
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
                className="brief-field flex-1 px-3 py-1.5 text-[13px]"
              />
              <button
                onClick={saveKey}
                disabled={!keyDraft.trim()}
                className="brief-button-primary px-3 py-1.5 text-xs disabled:opacity-40"
              >
                Salva
              </button>
            </div>
            <span className="block text-[11px] leading-snug text-ink-muted">
              Salvata in un file leggibile solo dal tuo utente, dentro la
              cartella dati di Brief.
            </span>
          </label>

          <button
            onClick={provaConnessione}
            disabled={testing || !keyPresent}
            className="brief-button px-3 py-1.5 text-xs disabled:opacity-40"
          >
            {testing ? <Spinner label="Prova in corso…" /> : "Prova la connessione"}
          </button>
        </section>

        <section className="space-y-3">
          <div>
            <h3 className="text-sm font-medium">Taglio del report</h3>
            <p className="text-xs text-ink-muted">
              Vale per i report generati d'ora in poi.
            </p>
          </div>

          <div className="grid grid-cols-2 gap-2">
            {REPORT_STYLES.map((option) => (
              <button
                key={option.value}
                onClick={() =>
                  save({ ...settings, report_style: option.value as ReportStyle })
                }
                className={`rounded-lg border px-3 py-2 text-left transition-colors ${
                  settings.report_style === option.value
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

          <div className="flex gap-2 pt-1">
            {REPORT_LENGTHS.map((option) => (
              <button
                key={option.value}
                onClick={() =>
                  save({
                    ...settings,
                    report_length: option.value as ReportLength,
                  })
                }
                className={`flex-1 rounded-lg border px-3 py-2 text-center transition-colors ${
                  settings.report_length === option.value
                    ? "border-accent bg-accent-soft"
                    : "border-edge hover:bg-surface-raised"
                }`}
              >
                <span className="block text-xs font-medium">{option.label}</span>
                <span className="block text-[11px] text-ink-muted">
                  {option.detail}
                </span>
              </button>
            ))}
          </div>

          <label className="block space-y-1.5">
            <span className="text-xs text-ink-muted">
              Istruzioni aggiuntive (facoltative)
            </span>
            <textarea
              value={settings.report_notes}
              onChange={(event) =>
                setLocalSettings({ ...settings, report_notes: event.target.value })
              }
              onBlur={() => save(settings)}
              rows={2}
              placeholder="es. dai sempre risalto alle scadenze, oppure scrivi in terza persona"
              className="brief-field w-full resize-none px-3 py-1.5 text-[13px]"
            />
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
                {model.complete && (
                  <button
                    onClick={() => verifica(model.file_name, model.label)}
                    disabled={checking !== null}
                    className="brief-button shrink-0 px-2.5 py-1 text-[11px] text-ink-muted disabled:opacity-40"
                  >
                    {checking === model.file_name ? "Verifico…" : "Verifica"}
                  </button>
                )}
                {model.on_disk > 0 && (
                  <button
                    onClick={() => removeModel(model.file_name)}
                    className="brief-button shrink-0 px-2.5 py-1 text-[11px] text-ink-muted hover:text-live"
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

          <button
            onClick={() => revealDataFolder().catch(() => undefined)}
            className="brief-button px-3 py-1.5 text-xs"
          >
            Apri la cartella dei dati
          </button>
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
