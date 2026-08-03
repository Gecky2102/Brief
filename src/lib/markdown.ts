import { KIND_LABELS, type Analysis, type Segment, type Session } from "./db";

function formatDuration(ms: number): string {
  const totalSeconds = Math.round(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes} min ${String(seconds).padStart(2, "0")} s`;
}

function formatStamp(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export function speakerOf(
  track: "mic" | "system",
  label?: string | null,
): string {
  if (track === "mic") return "Io";
  return label?.trim() || "Interlocutore";
}

/// Riepilogo di chi ha parlato e quanto: apre il documento esportato dando
/// subito il contesto di chi c'era.
function speakerSummary(segments: Segment[]): string {
  const durate = new Map<string, number>();
  for (const segment of segments) {
    const nome = speakerOf(segment.track, segment.speaker_label);
    durate.set(
      nome,
      (durate.get(nome) ?? 0) + Math.max(segment.end_ms - segment.start_ms, 0),
    );
  }
  if (durate.size < 2) return "";

  const totale = [...durate.values()].reduce((a, b) => a + b, 0);
  const righe = [...durate.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(
      ([nome, ms]) =>
        `| ${nome} | ${Math.round(ms / 60000)} min | ${Math.round((ms / totale) * 100)}% |`,
    );

  return [
    "## Interventi",
    "",
    "| Voce | Durata | Quota |",
    "|---|---|---|",
    ...righe,
    "",
  ].join("\n");
}

export function toMarkdown(
  session: Session,
  segments: Segment[],
  analysis: Analysis | null,
): string {
  const started = new Date(session.started_at);
  const header = [
    `# ${session.title}`,
    "",
    `- **Data**: ${started.toLocaleString("it-IT")}`,
    `- **Durata**: ${formatDuration(session.duration_ms)}`,
    `- **Tipo**: ${KIND_LABELS[session.kind]}`,
    "",
    "---",
    "",
  ].join("\n");

  // Il report è già Markdown completo: si innesta così com'è, togliendo il
  // suo titolo per non averne due.
  const report = analysis?.report
    ? `${analysis.report.replace(/^#\s+.*\n/, "")}\n\n---\n\n`
    : "";

  const transcript = segments
    .map(
      (segment) =>
        `**${speakerOf(segment.track, segment.speaker_label)}** \`${formatStamp(segment.start_ms)}\` — ${segment.text}`,
    )
    .join("\n\n");

  return `${header}${report}${speakerSummary(segments)}\n## Trascrizione integrale\n\n${transcript}\n`;
}

export function fileNameFor(session: Session): string {
  const slug = session.title
    .toLowerCase()
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 60);
  return `${slug || "sessione"}.md`;
}
