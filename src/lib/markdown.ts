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

function list(title: string, items: string[]): string {
  if (items.length === 0) return "";
  return `\n## ${title}\n\n${items.map((item) => `- ${item}`).join("\n")}\n`;
}

export function speakerOf(track: "mic" | "system"): string {
  return track === "mic" ? "Io" : "Interlocutore";
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
  ].join("\n");

  const summary = analysis?.summary
    ? `## Riassunto\n\n${analysis.summary}\n`
    : "";

  const sections = analysis
    ? list("Decisioni", analysis.decisions) +
      list("Da fare", analysis.actions) +
      list("Domande aperte", analysis.questions)
    : "";

  const transcript = segments
    .map(
      (segment) =>
        `**${speakerOf(segment.track)}** \`${formatStamp(segment.start_ms)}\` — ${segment.text}`,
    )
    .join("\n\n");

  return `${header}${summary}${sections}\n## Trascrizione\n\n${transcript}\n`;
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
