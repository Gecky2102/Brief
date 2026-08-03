-- Le voci riconosciute in una sessione. `label` è il nome dato dall'utente,
-- `cluster_index` è il gruppo assegnato automaticamente dall'impronta vocale.
CREATE TABLE speakers (
  id            INTEGER PRIMARY KEY,
  session_id    INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  cluster_index INTEGER NOT NULL,
  label         TEXT    NOT NULL,
  UNIQUE (session_id, cluster_index)
);

ALTER TABLE segments ADD COLUMN speaker_id INTEGER REFERENCES speakers(id);

CREATE INDEX idx_segments_speaker ON segments(speaker_id);
