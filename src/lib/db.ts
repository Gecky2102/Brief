import Database from "@tauri-apps/plugin-sql";

export type SessionKind =
  | "unknown"
  | "work_call"
  | "meeting"
  | "lecture"
  | "interview"
  | "casual";

export type Session = {
  id: number;
  title: string;
  kind: SessionKind;
  started_at: string;
  ended_at: string | null;
  duration_ms: number;
  audio_path: string | null;
};

let instance: Database | null = null;

export async function db(): Promise<Database> {
  if (!instance) {
    instance = await Database.load("sqlite:brief.db");
  }
  return instance;
}

export async function listSessions(): Promise<Session[]> {
  const conn = await db();
  return conn.select<Session[]>(
    "SELECT id, title, kind, started_at, ended_at, duration_ms, audio_path FROM sessions ORDER BY started_at DESC",
  );
}

export async function searchSessions(query: string): Promise<Session[]> {
  const conn = await db();
  return conn.select<Session[]>(
    `SELECT s.id, s.title, s.kind, s.started_at, s.ended_at, s.duration_ms, s.audio_path
     FROM segments_fts f
     JOIN segments g ON g.id = f.rowid
     JOIN sessions s ON s.id = g.session_id
     WHERE segments_fts MATCH $1
     GROUP BY s.id
     ORDER BY s.started_at DESC`,
    [query],
  );
}
