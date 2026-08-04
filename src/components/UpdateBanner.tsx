import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import Spinner from "./Spinner";

type Stato = "inattivo" | "disponibile" | "scarico" | "pronto" | "errore";

/// Un aggiornamento non si installa mai da solo: chi sta registrando o
/// analizzando una riunione non vuole che l'app si riavvii sotto le mani.
export default function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [stato, setStato] = useState<Stato>("inattivo");
  const [scaricato, setScaricato] = useState(0);
  const [totale, setTotale] = useState(0);
  const [errore, setErrore] = useState<string | null>(null);
  const [ignorato, setIgnorato] = useState(false);

  useEffect(() => {
    check()
      .then((disponibile) => {
        if (disponibile) {
          setUpdate(disponibile);
          setStato("disponibile");
        }
      })
      .catch(() => undefined);
  }, []);

  async function installa() {
    if (!update) return;
    setStato("scarico");
    setErrore(null);
    try {
      await update.downloadAndInstall((evento) => {
        if (evento.event === "Started") {
          setTotale(evento.data.contentLength ?? 0);
        } else if (evento.event === "Progress") {
          setScaricato((corrente) => corrente + evento.data.chunkLength);
        } else if (evento.event === "Finished") {
          setStato("pronto");
        }
      });
      setStato("pronto");
    } catch (causa: unknown) {
      setErrore(String(causa));
      setStato("errore");
    }
  }

  if (ignorato || stato === "inattivo" || !update) return null;

  const percentuale =
    totale > 0 ? Math.min(100, Math.round((scaricato / totale) * 100)) : 0;

  return (
    <div className="border-b border-edge bg-accent-soft px-3 py-2">
      {stato === "disponibile" && (
        <div className="space-y-1.5">
          <p className="text-[11px] leading-snug">
            <strong className="font-medium">Versione {update.version}</strong>{" "}
            disponibile.
          </p>
          <div className="flex gap-1.5">
            <button
              onClick={installa}
              className="brief-button-primary px-2.5 py-1 text-[11px]"
            >
              Aggiorna
            </button>
            <button
              onClick={() => setIgnorato(true)}
              className="brief-button px-2.5 py-1 text-[11px]"
            >
              Più tardi
            </button>
          </div>
        </div>
      )}

      {stato === "scarico" && (
        <div className="space-y-1.5">
          <span className="flex items-center gap-2 text-[11px]">
            <Spinner />
            Scarico l'aggiornamento {percentuale > 0 && `${percentuale}%`}
          </span>
          <div className="h-1 overflow-hidden rounded-full bg-surface-sunken">
            <div
              className="h-full rounded-full bg-accent transition-[width]"
              style={{ width: `${percentuale}%` }}
            />
          </div>
        </div>
      )}

      {stato === "pronto" && (
        <div className="space-y-1.5">
          <p className="text-[11px] leading-snug">
            Aggiornamento pronto. Brief va riavviata per applicarlo.
          </p>
          <div className="flex gap-1.5">
            <button
              onClick={() => relaunch()}
              className="brief-button-primary px-2.5 py-1 text-[11px]"
            >
              Riavvia ora
            </button>
            <button
              onClick={() => setIgnorato(true)}
              className="brief-button px-2.5 py-1 text-[11px]"
            >
              Al prossimo avvio
            </button>
          </div>
        </div>
      )}

      {stato === "errore" && (
        <p className="text-[11px] leading-snug text-live">
          Aggiornamento non riuscito: {errore}
        </p>
      )}
    </div>
  );
}
