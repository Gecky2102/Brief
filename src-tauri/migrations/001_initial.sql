CREATE TABLE sessions (
  id          INTEGER PRIMARY KEY,
  title       TEXT    NOT NULL,
  kind        TEXT    NOT NULL DEFAULT 'unknown',
  started_at  TEXT    NOT NULL,
  ended_at    TEXT,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  audio_path  TEXT
);

-- track: 'mic' (utente) oppure 'system' (interlocutori), tenuti separati
-- per avere la diarizzazione senza costi aggiuntivi.
CREATE TABLE segments (
  id         INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  track      TEXT    NOT NULL CHECK (track IN ('mic', 'system')),
  start_ms   INTEGER NOT NULL,
  end_ms     INTEGER NOT NULL,
  text       TEXT    NOT NULL
);

CREATE INDEX idx_segments_session ON segments(session_id, start_ms);

CREATE TABLE analyses (
  id         INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  kind       TEXT    NOT NULL,
  content    TEXT    NOT NULL,
  model      TEXT    NOT NULL,
  created_at TEXT    NOT NULL
);

CREATE INDEX idx_analyses_session ON analyses(session_id);

CREATE VIRTUAL TABLE segments_fts USING fts5(
  text,
  content = 'segments',
  content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER segments_fts_insert AFTER INSERT ON segments BEGIN
  INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER segments_fts_delete AFTER DELETE ON segments BEGIN
  INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;

CREATE TRIGGER segments_fts_update AFTER UPDATE ON segments BEGIN
  INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
END;
