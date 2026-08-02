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

export function modelsStatus(): Promise<ModelsStatus> {
  return invoke<ModelsStatus>("models_status");
}

export function analyzeSession(
  lines: { speaker: string; text: string }[],
): Promise<Analysis> {
  return invoke<Analysis>("analyze_session", { lines });
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

export function exportAudio(directory: string): Promise<boolean> {
  return invoke<boolean>("export_audio", { directory });
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
