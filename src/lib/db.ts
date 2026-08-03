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
  folder_id: number | null;
};

export type Segment = {
  id: number;
  track: "mic" | "system";
  start_ms: number;
  end_ms: number;
  text: string;
  speaker_id: number | null;
  speaker_label: string | null;
  excluded: number;
};

export type Folder = {
  id: number;
  name: string;
  color: string;
};

export type Speaker = {
  id: number;
  cluster_index: number;
  label: string;
};

export type Analysis = {
  kind: SessionKind;
  title: string;
  summary: string;
  /// Il report completo in Markdown.
  report: string;
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

export async function listSessions(folderId?: number | null): Promise<Session[]> {
  const conn = await db();
  if (folderId === undefined) {
    return conn.select<Session[]>(
      `SELECT id, title, kind, started_at, ended_at, duration_ms, audio_path,
              folder_id
       FROM sessions ORDER BY started_at DESC`,
    );
  }
  return conn.select<Session[]>(
    `SELECT id, title, kind, started_at, ended_at, duration_ms, audio_path,
            folder_id
     FROM sessions
     WHERE folder_id IS $1
     ORDER BY started_at DESC`,
    [folderId],
  );
}

export async function listFolders(): Promise<(Folder & { count: number })[]> {
  const conn = await db();
  return conn.select<(Folder & { count: number })[]>(
    `SELECT f.id, f.name, f.color, COUNT(s.id) AS count
     FROM folders f
     LEFT JOIN sessions s ON s.folder_id = f.id
     GROUP BY f.id ORDER BY f.name`,
  );
}

export async function createFolder(name: string): Promise<number> {
  const conn = await db();
  const result = await conn.execute("INSERT INTO folders (name) VALUES ($1)", [
    name,
  ]);
  return Number(result.lastInsertId ?? 0);
}

export async function deleteFolder(id: number): Promise<void> {
  const conn = await db();
  // Le sessioni non si perdono: tornano semplicemente fuori dalle cartelle.
  await conn.execute("UPDATE sessions SET folder_id = NULL WHERE folder_id = $1", [id]);
  await conn.execute("DELETE FROM folders WHERE id = $1", [id]);
}

export async function moveSession(
  sessionId: number,
  folderId: number | null,
): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE sessions SET folder_id = $1 WHERE id = $2", [
    folderId,
    sessionId,
  ]);
}

export type SearchHit = Session & { excerpt: string; hits: number };

/// Oltre alle sessioni restituisce un estratto del punto in cui compare il
/// termine: scorrere un elenco di titoli non dice se il risultato è utile.
export async function searchWithExcerpts(query: string): Promise<SearchHit[]> {
  const conn = await db();
  return conn.select<SearchHit[]>(
    `SELECT s.id, s.title, s.kind, s.started_at, s.ended_at, s.duration_ms,
            s.audio_path,
            snippet(segments_fts, 0, '«', '»', '…', 12) AS excerpt,
            COUNT(*) AS hits
     FROM segments_fts f
     JOIN segments g ON g.id = f.rowid
     JOIN sessions s ON s.id = g.session_id
     WHERE segments_fts MATCH $1
     GROUP BY s.id
     ORDER BY s.started_at DESC`,
    [query],
  );
}

export async function searchSessions(query: string): Promise<Session[]> {
  const conn = await db();
  return conn.select<Session[]>(
    `SELECT s.id, s.title, s.kind, s.started_at, s.ended_at, s.duration_ms,
            s.audio_path, s.folder_id
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

/// Restituisce la voce corrispondente al gruppo, creandola al primo incontro
/// con un nome provvisorio numerato.
async function ensureSpeaker(
  sessionId: number,
  clusterIndex: number,
): Promise<number> {
  const conn = await db();
  const esistenti = await conn.select<{ id: number }[]>(
    "SELECT id FROM speakers WHERE session_id = $1 AND cluster_index = $2",
    [sessionId, clusterIndex],
  );
  if (esistenti.length > 0) return esistenti[0].id;

  const result = await conn.execute(
    "INSERT INTO speakers (session_id, cluster_index, label) VALUES ($1, $2, $3)",
    [sessionId, clusterIndex, `Voce ${clusterIndex + 1}`],
  );
  return Number(result.lastInsertId ?? 0);
}

export async function addSegment(input: {
  sessionId: number;
  track: "mic" | "system";
  startMs: number;
  endMs: number;
  text: string;
  speaker: number | null;
}): Promise<void> {
  const conn = await db();
  const speakerId =
    input.speaker === null
      ? null
      : await ensureSpeaker(input.sessionId, input.speaker);

  await conn.execute(
    `INSERT INTO segments (session_id, track, start_ms, end_ms, text, speaker_id)
     VALUES ($1, $2, $3, $4, $5, $6)`,
    [
      input.sessionId,
      input.track,
      input.startMs,
      input.endMs,
      input.text,
      speakerId,
    ],
  );
}

export async function listSpeakers(sessionId: number): Promise<Speaker[]> {
  const conn = await db();
  return conn.select<Speaker[]>(
    "SELECT id, cluster_index, label FROM speakers WHERE session_id = $1 ORDER BY cluster_index",
    [sessionId],
  );
}

export async function renameSpeaker(id: number, label: string): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE speakers SET label = $1 WHERE id = $2", [label, id]);
}

/// Sposta tutti i segmenti di una voce su un'altra ed elimina quella svuotata:
/// serve quando il riconoscimento ha diviso in due la stessa persona.
export async function mergeSpeakers(from: number, into: number): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE segments SET speaker_id = $1 WHERE speaker_id = $2", [
    into,
    from,
  ]);
  await conn.execute("DELETE FROM speakers WHERE id = $1", [from]);
}

/// Esclude una riga dal report senza cancellarla: capita di registrare
/// passaggi personali che non devono finire in un documento condiviso.
export async function toggleSegmentExcluded(
  id: number,
  excluded: boolean,
): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE segments SET excluded = $1 WHERE id = $2", [
    excluded ? 1 : 0,
    id,
  ]);
}

export async function assignSegment(
  segmentId: number,
  speakerId: number | null,
): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE segments SET speaker_id = $1 WHERE id = $2", [
    speakerId,
    segmentId,
  ]);
}

export async function listSegments(sessionId: number): Promise<Segment[]> {
  const conn = await db();
  return conn.select<Segment[]>(
    `SELECT g.id, g.track, g.start_ms, g.end_ms, g.text, g.speaker_id,
            COALESCE(g.excluded, 0) AS excluded,
            p.label AS speaker_label
     FROM segments g
     LEFT JOIN speakers p ON p.id = g.speaker_id
     WHERE g.session_id = $1 ORDER BY g.start_ms`,
    [sessionId],
  );
}

/// Whisper sbaglia parole e nomi propri: poterli correggere a mano evita che
/// l'errore si propaghi al riassunto e alla ricerca.
export async function updateSegmentText(
  id: number,
  text: string,
): Promise<void> {
  const conn = await db();
  await conn.execute("UPDATE segments SET text = $1 WHERE id = $2", [text, id]);
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

/// Statistiche complessive dell'archivio, per la schermata iniziale.
export async function saveEmbeddings(
  entries: [number, number[]][],
): Promise<void> {
  const conn = await db();
  for (const [segmentId, vector] of entries) {
    await conn.execute(
      "INSERT OR REPLACE INTO embeddings (segment_id, vector) VALUES ($1, $2)",
      [segmentId, JSON.stringify(vector)],
    );
  }
}

/// Righe non ancora indicizzate: l'indicizzazione è incrementale, così una
/// nuova sessione non costringe a rifare tutto l'archivio.
export async function segmentsWithoutEmbedding(): Promise<
  { id: number; text: string }[]
> {
  const conn = await db();
  return conn.select<{ id: number; text: string }[]>(
    `SELECT g.id, g.text FROM segments g
     LEFT JOIN embeddings e ON e.segment_id = g.id
     WHERE e.segment_id IS NULL AND LENGTH(g.text) > 15
     LIMIT 400`,
  );
}

export async function loadEmbeddings(): Promise<[number, number[]][]> {
  const conn = await db();
  const righe = await conn.select<{ segment_id: number; vector: string }[]>(
    "SELECT segment_id, vector FROM embeddings",
  );
  return righe.map((riga) => [riga.segment_id, JSON.parse(riga.vector)]);
}

export async function sessionsForSegments(
  ids: number[],
): Promise<(Session & { excerpt: string })[]> {
  if (ids.length === 0) return [];
  const conn = await db();
  const elenco = ids.join(",");
  return conn.select<(Session & { excerpt: string })[]>(
    `SELECT s.id, s.title, s.kind, s.started_at, s.ended_at, s.duration_ms,
            s.audio_path, s.folder_id,
            (SELECT text FROM segments WHERE id IN (${elenco})
              AND session_id = s.id LIMIT 1) AS excerpt
     FROM sessions s
     WHERE s.id IN (SELECT session_id FROM segments WHERE id IN (${elenco}))
     ORDER BY s.started_at DESC`,
  );
}

export async function archiveStats(): Promise<{
  sessions: number;
  minutes: number;
  words: number;
  reports: number;
}> {
  const conn = await db();
  const righe = await conn.select<
    { sessions: number; minutes: number; words: number; reports: number }[]
  >(
    `SELECT
       (SELECT COUNT(*) FROM sessions) AS sessions,
       (SELECT COALESCE(SUM(duration_ms), 0) / 60000 FROM sessions) AS minutes,
       (SELECT COALESCE(SUM(LENGTH(text) - LENGTH(REPLACE(text, ' ', '')) + 1), 0)
        FROM segments) AS words,
       (SELECT COUNT(*) FROM analyses) AS reports`,
  );
  return righe[0] ?? { sessions: 0, minutes: 0, words: 0, reports: 0 };
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
