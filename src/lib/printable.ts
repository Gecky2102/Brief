import type { Analysis, Session } from "./db";
import { KIND_LABELS } from "./db";

/// L'HTML del documento va costruito qui e non preso dal DOM: la pagina
/// contiene barre laterali, pulsanti e stili pensati per lo schermo, mentre
/// il PDF deve avere solo il documento con misure da carta.
export function toPrintableHtml(
  session: Session,
  analysis: Analysis,
  markdownToHtml: (markdown: string) => string,
): string {
  const data = new Date(session.started_at).toLocaleString("it-IT", {
    dateStyle: "long",
    timeStyle: "short",
  });
  const durata = `${Math.round(session.duration_ms / 60000)} min`;

  return `<!doctype html>
<html lang="it"><head><meta charset="utf-8"><style>
  @page { margin: 20mm 18mm; }
  * { box-sizing: border-box; }
  body {
    font-family: -apple-system, "SF Pro Text", "Helvetica Neue", sans-serif;
    font-size: 10.5pt; line-height: 1.6; color: #1d1d1f; margin: 0;
    -webkit-font-smoothing: antialiased;
  }
  header { border-bottom: 1px solid #d2d2d7; padding-bottom: 10pt; margin-bottom: 18pt; }
  header .meta { color: #6e6e73; font-size: 9pt; margin-top: 4pt; }
  h1 { font-size: 22pt; line-height: 1.2; margin: 0; letter-spacing: -0.02em; }
  h2 {
    font-size: 14pt; margin: 20pt 0 6pt; letter-spacing: -0.01em;
    break-after: avoid; page-break-after: avoid;
  }
  h3 { font-size: 11.5pt; margin: 14pt 0 4pt; break-after: avoid; }
  p { margin: 7pt 0; }
  ul, ol { margin: 7pt 0; padding-left: 16pt; }
  li { margin: 3pt 0; break-inside: avoid; }
  strong { font-weight: 600; }
  table {
    width: 100%; border-collapse: collapse; margin: 10pt 0;
    font-size: 9.5pt; break-inside: avoid;
  }
  th {
    text-align: left; font-size: 8pt; text-transform: uppercase;
    letter-spacing: 0.04em; color: #6e6e73; font-weight: 600;
    border-bottom: 1px solid #d2d2d7; padding: 5pt 7pt; background: #f5f5f7;
  }
  td { border-bottom: 1px solid #e8e8ed; padding: 5pt 7pt; vertical-align: top; }
  blockquote {
    margin: 10pt 0; padding-left: 10pt; border-left: 2pt solid #0a6cff;
    color: #6e6e73;
  }
  code { font-family: "SF Mono", monospace; font-size: 9pt; background: #f5f5f7;
    padding: 1pt 3pt; border-radius: 3pt; }
  hr { border: none; border-top: 1px solid #e8e8ed; margin: 16pt 0; }
</style></head>
<body>
  <header>
    <h1>${escapeHtml(session.title)}</h1>
    <div class="meta">${escapeHtml(data)} · ${durata} · ${KIND_LABELS[session.kind]}</div>
  </header>
  ${markdownToHtml(stripTitle(analysis.report))}
</body></html>`;
}

/// Il titolo sta già nell'intestazione: ripeterlo raddoppierebbe.
function stripTitle(markdown: string): string {
  return markdown.replace(/^#\s+.*\n/, "");
}

function escapeHtml(testo: string): string {
  return testo
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
