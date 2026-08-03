import type { Provider, ReportLength, ReportStyle } from "./recorder";

/// Modelli suggeriti per fornitore. Il campo resta comunque scrivibile a mano:
/// i cataloghi cambiano in fretta e non voglio bloccare scelte nuove.
export const MODELS: Record<Provider, { id: string; note: string }[]> = {
  anthropic: [
    { id: "claude-opus-5", note: "il più capace, per report lunghi e complessi" },
    { id: "claude-sonnet-5", note: "equilibrio fra qualità e costo" },
    { id: "claude-haiku-4-5", note: "rapido ed economico" },
  ],
  openai: [
    { id: "gpt-5", note: "il più capace" },
    { id: "gpt-5-mini", note: "più rapido ed economico" },
    { id: "o4-mini", note: "ragionamento, utile su contenuti tecnici" },
  ],
  google: [
    { id: "gemini-2.5-pro", note: "finestra molto ampia, adatto a riunioni lunghe" },
    { id: "gemini-2.5-flash", note: "rapido ed economico" },
  ],
  openrouter: [
    { id: "anthropic/claude-sonnet-5", note: "Claude via OpenRouter" },
    { id: "openai/gpt-5", note: "GPT via OpenRouter" },
    { id: "google/gemini-2.5-pro", note: "Gemini via OpenRouter" },
    { id: "deepseek/deepseek-chat", note: "molto economico" },
    { id: "meta-llama/llama-3.3-70b-instruct", note: "aperto, buon rapporto qualità/prezzo" },
  ],
  compatible: [],
};

export const REPORT_STYLES: {
  value: ReportStyle;
  label: string;
  detail: string;
}[] = [
  {
    value: "auto",
    label: "Automatico",
    detail: "Riconosce da solo se è una riunione, una lezione o un'intervista",
  },
  {
    value: "meeting",
    label: "Riunione",
    detail: "Temi, decisioni, attività, punti aperti, rischi",
  },
  {
    value: "executive",
    label: "Sintesi direzionale",
    detail: "L'essenziale in cima, il dettaglio in fondo",
  },
  {
    value: "minutes",
    label: "Verbale",
    detail: "Ordine del giorno, svolgimento, deliberazioni, impegni",
  },
  {
    value: "lecture",
    label: "Lezione",
    detail: "Concetti, definizioni, esempi svolti, cosa studiare",
  },
  {
    value: "interview",
    label: "Intervista",
    detail: "Temi, citazioni testuali, fatti e cifre",
  },
  {
    value: "standup",
    label: "Punto di avanzamento",
    detail: "Per persona: fatto, in corso, impedimenti",
  },
  {
    value: "brainstorm",
    label: "Brainstorming",
    detail: "Idee, confronto pro e contro, direzioni promettenti",
  },
];

export const REPORT_LENGTHS: {
  value: ReportLength;
  label: string;
  detail: string;
}[] = [
  { value: "brief", label: "Sintetico", detail: "600-1000 parole" },
  { value: "standard", label: "Standard", detail: "1500-3000 parole" },
  { value: "deep", label: "Approfondito", detail: "3500-6000 parole" },
];
