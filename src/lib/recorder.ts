import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Track = "mic" | "system";

export type LevelEvent = {
  track: Track;
  rms: number;
  elapsed_ms: number;
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

export function startRecording(): Promise<StartedRecording> {
  return invoke<StartedRecording>("start_recording");
}

export function stopRecording(): Promise<FinishedRecording> {
  return invoke<FinishedRecording>("stop_recording");
}

export function isRecording(): Promise<boolean> {
  return invoke<boolean>("is_recording");
}

export function onLevel(handler: (event: LevelEvent) => void): Promise<UnlistenFn> {
  return listen<LevelEvent>("audio://level", (event) => handler(event.payload));
}
