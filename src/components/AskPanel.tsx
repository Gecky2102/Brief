import { useEffect, useRef, useState } from "react";
import ReportView from "./ReportView";
import Spinner from "./Spinner";
import { askTranscript, onAnalysisProgress } from "../lib/recorder";

type Props = {
  lines: { speaker: string; text: string }[];
};

type Scambio = { domanda: string; risposta: string };

const SUGGERIMENTI = [
  "Quali scadenze sono state fissate?",
  "Su cosa non c'è stato accordo?",
  "Cosa devo fare io?",
  "Quali numeri sono stati citati?",
];

/// Interrogare la trascrizione invece di rileggerla: su una riunione di
/// un'ora è molto più rapido che cercare a mano.
export default function AskPanel({ lines }: Props) {
  const [domanda, setDomanda] = useState("");
  const [scambi, setScambi] = useState<Scambio[]>([]);
  const [inCorso, setInCorso] = useState(false);
  const [parziale, setParziale] = useState("");
  const [errore, setErrore] = useState<string | null>(null);
  const fondo = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const sottoscrizione = onAnalysisProgress((event) => {
      if (inCorso) setParziale(event.preview);
    });
    return () => {
      sottoscrizione.then((annulla) => annulla());
    };
  }, [inCorso]);

  useEffect(() => {
    fondo.current?.scrollIntoView({ behavior: "smooth" });
  }, [scambi.length, parziale]);

  async function chiedi(testo: string) {
    if (!testo.trim() || inCorso) return;
    setInCorso(true);
    setErrore(null);
    setParziale("");
    setDomanda("");

    try {
      const risposta = await askTranscript(lines, testo);
      setScambi((precedenti) => [...precedenti, { domanda: testo, risposta }]);
    } catch (causa: unknown) {
      setErrore(String(causa));
    } finally {
      setInCorso(false);
      setParziale("");
    }
  }

  return (
    <div className="mx-auto max-w-[46rem] space-y-4 pb-8">
      {scambi.length === 0 && !inCorso && (
        <div className="space-y-3 py-6">
          <p className="text-center text-xs leading-relaxed text-ink-muted">
            Chiedi qualcosa su questa conversazione. La risposta viene solo da
            ciò che è stato detto.
          </p>
          <div className="flex flex-wrap justify-center gap-1.5">
            {SUGGERIMENTI.map((suggerimento) => (
              <button
                key={suggerimento}
                onClick={() => chiedi(suggerimento)}
                className="brief-button px-2.5 py-1 text-[11px]"
              >
                {suggerimento}
              </button>
            ))}
          </div>
        </div>
      )}

      {scambi.map((scambio, indice) => (
        <div key={indice} className="space-y-2">
          <p className="ml-auto w-fit max-w-[80%] rounded-2xl bg-accent px-3.5 py-2 text-[13px] text-white">
            {scambio.domanda}
          </p>
          <div className="rounded-2xl border border-edge bg-surface-raised/50 px-4 py-3">
            <ReportView markdown={scambio.risposta} />
          </div>
        </div>
      ))}

      {inCorso && (
        <div className="space-y-2">
          <div className="rounded-2xl border border-edge bg-surface-raised/50 px-4 py-3">
            {parziale ? (
              <ReportView markdown={parziale} />
            ) : (
              <Spinner label="Sto leggendo la trascrizione…" />
            )}
          </div>
        </div>
      )}

      {errore && (
        <p className="rounded-md border border-live/40 bg-live/10 px-3 py-2 text-xs text-live">
          {errore}
        </p>
      )}

      <div className="sticky bottom-0 flex gap-2 bg-[var(--content)] py-2">
        <input
          value={domanda}
          onChange={(event) => setDomanda(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void chiedi(domanda);
          }}
          placeholder="Fai una domanda su questa conversazione"
          className="brief-field flex-1 px-3 py-2 text-[13px]"
        />
        <button
          onClick={() => chiedi(domanda)}
          disabled={!domanda.trim() || inCorso}
          className="brief-button-primary px-4 py-2 text-xs disabled:opacity-40"
        >
          Chiedi
        </button>
      </div>

      <div ref={fondo} />
    </div>
  );
}
