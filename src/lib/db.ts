import Database from "@tauri-apps/plugin-sql";

export type SessionKind =
  | "unknown"
  | "work_call"
  | "meeting"
  | "lecture"
  | "interview"
  | "casual";

export const KIND_LABELS: Record<SessionKind, string> = {
  unknown: "Non classificata",
  work_call: "Call di lavoro",
  meeting: "Riunione",
  lecture: "Lezione",
  interview: "Intervista",
  casual: "Chiacchierata",
};

export type Session = {
  id: number;
  title: string;
  kind: SessionKind;
  started_at: string;
  ended_at: string | null;
  duration_ms: number;
  audio_path: string | null;
};

export type Segment = {
  id: number;
  track: "mic" | "system";
  start_ms: number;
  end_ms: number;
  text: string;
};

export type Analysis = {
  kind: SessionKind;
  title: string;
  summary: string;
  decisions: string[];
  actions: string[];
  questions: string[];
};

let instance: Database | null = null;

export async function db(): Promise<Database> {
  if (!instance) {
    instance = await Database.load("sqlite:brief.db");
  }
  return instance;
}

/// Se l'app viene chiusa mentre registra, la sessione resta senza `ended_at`.
/// All'avvio o la si chiude con quello che ha, o la si elimina se è vuota.
export async function reconcileOrphanSessions(): Promise<void> {
  const conn = await db();
  await conn.execute(
    `DELETE FROM sessions WHERE ended_at IS NULL
     AND id NOT IN (SELECT DISTINCT session_id FROM segments)`,
  );
  await conn.execute(
    `UPDATE sessions SET ended_at = started_at,
       title = 'Sessione interrotta ' || substr(started_at, 1, 10)
     WHERE ended_at IS NULL`,
  );
}

export async function listSessions(): Promise<Session[]> {
  const conn = await db();
  return conn.select<Session[]>(
    `SELECT id, title, kind, started_at, ended_at, duration_ms, audio_path
     FROM sessions ORDER BY started_at DESC`,
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

export async function createSession(startedAt: string): Promise<number> {
  const conn = await db();
  const result = await conn.execute(
    `INSERT INTO sessions (title, kind, started_at, duration_ms)
     VALUES ($1, 'unknown', $2, 0)`,
    ["Registrazione in corso", startedAt],
  );
  return Number(result.lastInsertId ?? 0);
}

export async function finishSession(input: {
  id: number;
  title: string;
  endedAt: string;
  durationMs: number;
  audioPath: string;
}): Promise<void> {
  const conn = await db();
  await conn.execute(
    `UPDATE sessions SET title = $1, ended_at = $2, duration_ms = $3, audio_path = $4
     WHERE id = $5`,
    [input.title, input.endedAt, input.durationMs, input.audioPath, input.id],
  );
}

export async function deleteSession(id: number): Promise<void> {
  const conn = await db();
  await conn.execute("DELETE FROM segments WHERE session_id = $1", [id]);
  await conn.execute("DELETE FROM analyses WHERE session_id = $1", [id]);
  await conn.execute("DELETE FROM sessions WHERE id = $1", [id]);
}

export async function addSegment(input: {
  sessionId: number;
  track: "mic" | "system";
  startMs: number;
  endMs: number;
  text: string;
}): Promise<void> {
  const conn = await db();
  await conn.execute(
    `INSERT INTO segments (session_id, track, start_ms, end_ms, text)
     VALUES ($1, $2, $3, $4, $5)`,
    [input.sessionId, input.track, input.startMs, input.endMs, input.text],
  );
}

export async function listSegments(sessionId: number): Promise<Segment[]> {
  const conn = await db();
  return conn.select<Segment[]>(
    `SELECT id, track, start_ms, end_ms, text FROM segments
     WHERE session_id = $1 ORDER BY start_ms`,
    [sessionId],
  );
}

export async function setSessionKind(
  id: number,
  kind: SessionKind,
): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE sessions SET kind = $1 WHERE id = $2", [kind, id]);
}

export async function setSessionTitle(
  id: number,
  title: string,
): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE sessions SET title = $1 WHERE id = $2", [
    title,
    id,
  ]);
}

export async function saveAnalysis(
  sessionId: number,
  analysis: Analysis,
  model: string,
): Promise<void> {
  const conn = await db();
  await conn.execute("DELETE FROM analyses WHERE session_id = $1", [sessionId]);
  await conn.execute(
    `INSERT INTO analyses (session_id, kind, content, model, created_at)
     VALUES ($1, 'summary', $2, $3, $4)`,
    [sessionId, JSON.stringify(analysis), model, new Date().toISOString()],
  );
}

export async function loadAnalysis(
  sessionId: number,
): Promise<Analysis | null> {
  const conn = await db();
  const rows = await conn.select<{ content: string }[]>(
    "SELECT content FROM analyses WHERE session_id = $1 LIMIT 1",
    [sessionId],
  );
  if (rows.length === 0) return null;
  try {
    return JSON.parse(rows[0].content) as Analysis;
  } catch {
    return null;
  }
}
