import { useState } from "react";
import { setApiKey, setSettings, type Provider, type Settings } from "../lib/recorder";
import { MODELS } from "../lib/catalog";

type Props = {
  settings: Settings;
  onDone: () => void;
};

const SCELTE: { value: Provider; label: string; dove: string }[] = [
  { value: "anthropic", label: "Anthropic", dove: "console.anthropic.com" },
  { value: "openai", label: "OpenAI", dove: "platform.openai.com" },
  { value: "google", label: "Google", dove: "aistudio.google.com" },
  { value: "openrouter", label: "OpenRouter", dove: "openrouter.ai" },
];

/// Mostrato finché manca la chiave: senza, il pulsante «Genera report»
/// fallirebbe e basta, senza spiegare cosa fare.
export default function Welcome({ settings, onDone }: Props) {
  const [provider, setProvider] = useState<Provider>(settings.provider);
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function salva() {
    setSaving(true);
    setError(null);
    try {
      await setSettings({
        ...settings,
        provider,
        model: MODELS[provider][0]?.id ?? settings.model,
      });
      await setApiKey(key.trim());
      onDone();
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  }

  const scelto = SCELTE.find((s) => s.value === provider);

  return (
    <div className="mx-auto max-w-md space-y-5 py-6">
      <div className="space-y-1.5">
        <h2 className="text-lg font-semibold tracking-tight">
          Un passaggio e sei operativo
        </h2>
        <p className="text-xs leading-relaxed text-ink-muted">
          Registrazione e trascrizione funzionano già, e restano sul tuo computer.
          Per i report serve un servizio di intelligenza artificiale: scegli
          quale e incolla la sua chiave.
        </p>
      </div>

      <div className="grid grid-cols-2 gap-2">
        {SCELTE.map((scelta) => (
          <button
            key={scelta.value}
            onClick={() => setProvider(scelta.value)}
            className={`rounded-lg border px-3 py-2 text-left transition-colors ${
              provider === scelta.value
                ? "border-accent bg-accent-soft"
                : "border-edge hover:bg-surface-raised"
            }`}
          >
            <span className="block text-xs font-medium">{scelta.label}</span>
            <span className="block text-[11px] text-ink-muted">
              {scelta.dove}
            </span>
          </button>
        ))}
      </div>

      <div className="space-y-2">
        <input
          type="password"
          value={key}
          onChange={(event) => setKey(event.target.value)}
          placeholder={`Chiave di ${scelto?.label ?? "…"}`}
          className="brief-field w-full px-3 py-2 text-[13px]"
        />
        <div className="flex gap-2">
          <button
            onClick={salva}
            disabled={!key.trim() || saving}
            className="brief-button-primary flex-1 px-4 py-2 text-xs disabled:opacity-40"
          >
            {saving ? "Salvo…" : "Salva e inizia"}
          </button>
          <button onClick={onDone} className="brief-button px-4 py-2 text-xs">
            Più tardi
          </button>
        </div>
      </div>

      <p className="text-[11px] leading-relaxed text-ink-muted">
        La chiave resta sul tuo computer, in un file leggibile solo dal tuo utente.
        Al servizio scelto viene inviato il testo della trascrizione, mai
        l'audio.
      </p>

      {error && (
        <p className="rounded-md border border-live/40 bg-live/10 px-3 py-2 text-xs text-live">
          {error}
        </p>
      )}
    </div>
  );
}
