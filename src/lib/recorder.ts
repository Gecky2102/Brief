import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Analysis } from "./db";

export type Track = "mic" | "system";

export type LevelEvent = {
  track: Track;
  rms: number;
  elapsed_ms: number;
};

export type SegmentEvent = {
  session_id: number;
  track: Track;
  start_ms: number;
  end_ms: number;
  text: string;
  speaker: number | null;
};

export type DownloadProgress = {
  key: string;
  label: string;
  downloaded: number;
  total: number;
};

export type StartedRecording = {
  directory: string;
  started_at_ms: number;
};

export type FinishedRecording = {
  directory: string;
  duration_ms: number;
  mic_path: string;
  system_path: string;
};

export type ModelsStatus = {
  transcription: boolean;
  analysis: boolean;
};

export function startRecording(sessionId: number): Promise<StartedRecording> {
  return invoke<StartedRecording>("start_recording", { sessionId });
}

export function stopRecording(): Promise<FinishedRecording> {
  return invoke<FinishedRecording>("stop_recording");
}

export function isRecording(): Promise<boolean> {
  return invoke<boolean>("is_recording");
}

/// -1 = traccia di sistema non avviata, 0 = avviata ma silente, >0 = campioni ricevuti.
export function systemTrackHealth(): Promise<number> {
  return invoke<number>("system_track_health");
}

export type Quality = "fast" | "accurate";
export type Provider =
  | "anthropic"
  | "openai"
  | "google"
  | "openrouter"
  | "compatible";

export type ReportStyle =
  | "auto"
  | "meeting"
  | "executive"
  | "lecture"
  | "interview"
  | "standup"
  | "brainstorm"
  | "minutes";

export type ReportLength = "brief" | "standard" | "deep";

export type VoiceSensitivity = "low" | "medium" | "high";

export type Settings = {
  quality: Quality;
  provider: Provider;
  model: string;
  base_url: string;
  report_style: ReportStyle;
  report_length: ReportLength;
  report_notes: string;
  vocabulary: string;
  voice_sensitivity: VoiceSensitivity;
  expected_speakers: number;
};

export type ModelStatus = {
  key: string;
  label: string;
  file_name: string;
  bytes: number;
  on_disk: number;
  complete: boolean;
  in_use: boolean;
};

export type StorageReport = {
  models: ModelStatus[];
  used_bytes: number;
  free_bytes: number;
};

export type AnalysisProgress = {
  phase: "reading" | "writing";
  step: number;
  steps: number;
  preview: string;
  words: number;
};

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export function setSettings(settings: Settings): Promise<void> {
  return invoke<void>("set_settings", { settings });
}

export function hasApiKey(): Promise<boolean> {
  return invoke<boolean>("has_api_key");
}

export function setApiKey(key: string): Promise<void> {
  return invoke<void>("set_api_key", { key });
}

export function testProvider(): Promise<string> {
  return invoke<string>("test_provider");
}

export type SemanticHit = { segment_id: number; score: number };

export function embedSegments(texts: string[]): Promise<number[][]> {
  return invoke<number[][]>("embed_segments", { texts });
}

export function searchSemantic(
  query: string,
  candidates: [number, number[]][],
  limit: number,
): Promise<SemanticHit[]> {
  return invoke<SemanticHit[]>("search_semantic", {
    query,
    candidates,
    limit,
  });
}

export function semanticReady(): Promise<boolean> {
  return invoke<boolean>("semantic_ready");
}

export function storageReport(): Promise<StorageReport> {
  return invoke<StorageReport>("storage_report");
}

export function verifyModel(fileName: string): Promise<boolean> {
  return invoke<boolean>("verify_model", { fileName });
}

export function deleteModel(fileName: string): Promise<void> {
  return invoke<void>("delete_model", { fileName });
}

export function onAnalysisProgress(
  handler: (event: AnalysisProgress) => void,
): Promise<UnlistenFn> {
  return listen<AnalysisProgress>("analysis://progress", (event) =>
    handler(event.payload),
  );
}

export function modelsStatus(): Promise<ModelsStatus> {
  return invoke<ModelsStatus>("models_status");
}

export type SessionContext = {
  date: string;
  duration_minutes: number;
  speakers: string[];
};

export function analyzeSession(
  lines: { speaker: string; text: string }[],
  context: SessionContext,
): Promise<Analysis> {
  return invoke<Analysis>("analyze_session", { lines, context });
}

export type ImportedAudio = {
  file_name: string;
  duration_ms: number;
  directory: string;
};

export type ImportProgress = { done_ms: number; total_ms: number };

export function importAudio(sessionId: number): Promise<ImportedAudio> {
  return invoke<ImportedAudio>("import_audio", { sessionId });
}

export function onImportProgress(
  handler: (event: ImportProgress) => void,
): Promise<UnlistenFn> {
  return listen<ImportProgress>("import://progress", (event) =>
    handler(event.payload),
  );
}

export type AnalysisEstimate = {
  characters: number;
  chunks: number;
  calls: number;
};

export function askTranscript(
  lines: { speaker: string; text: string }[],
  question: string,
): Promise<string> {
  return invoke<string>("ask_transcript", { lines, question });
}

export function estimateAnalysis(
  lines: { speaker: string; text: string }[],
): Promise<AnalysisEstimate> {
  return invoke<AnalysisEstimate>("estimate_analysis", { lines });
}

export function compressRecording(directory: string): Promise<void> {
  return invoke<void>("compress_recording", { directory });
}

export function exportMarkdown(
  fileName: string,
  contents: string,
): Promise<boolean> {
  return invoke<boolean>("export_markdown", { fileName, contents });
}

export function exportMany(files: [string, string][]): Promise<number> {
  return invoke<number>("export_many", { files });
}

export function exportAudio(directory: string): Promise<boolean> {
  return invoke<boolean>("export_audio", { directory });
}

export function audioFile(directory: string): Promise<string | null> {
  return invoke<string | null>("audio_file", { directory });
}

export function revealDataFolder(): Promise<void> {
  return invoke<void>("reveal_data_folder");
}

export function deleteRecording(directory: string): Promise<void> {
  return invoke<void>("delete_recording", { directory });
}

export function onLevel(
  handler: (event: LevelEvent) => void,
): Promise<UnlistenFn> {
  return listen<LevelEvent>("audio://level", (event) => handler(event.payload));
}

export function onSegment(
  handler: (event: SegmentEvent) => void,
): Promise<UnlistenFn> {
  return listen<SegmentEvent>("transcript://segment", (event) =>
    handler(event.payload),
  );
}

export function onTranscriptError(
  handler: (message: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("transcript://error", (event) =>
    handler(event.payload),
  );
}

export function onDownloadProgress(
  handler: (event: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("model://progress", (event) =>
    handler(event.payload),
  );
}
