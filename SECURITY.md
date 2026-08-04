# Sicurezza

## Segnalare una vulnerabilità

Scrivi a **giacomomasiero.08@gmail.com** invece di aprire una issue pubblica.
Rispondo appena possibile.

## Come Brief tratta i tuoi dati

**L'audio non lascia mai il computer.** Registrazione, trascrizione,
riconoscimento delle voci e ricerca per significato avvengono in locale.

**Esce solo il testo della trascrizione**, e solo quando chiedi di generare un
documento: viene inviato al servizio che hai configurato. Puoi escludere
singole righe prima di generarlo.

**La chiave del servizio** è salvata in un file con permessi ristretti dentro
la cartella dati dell'applicazione, leggibile solo dal tuo utente. Non viene
mai mostrata nell'interfaccia né inclusa nei messaggi di errore.

**I modelli** vengono scaricati da HuggingFace e verificati contro l'hash
SHA-256 atteso: un file manomesso o troncato viene scartato senza essere usato.

## Cosa Brief non fa

Non raccoglie statistiche d'uso, non invia diagnostiche, non contatta alcun
server se non quello del servizio che hai scelto tu.
