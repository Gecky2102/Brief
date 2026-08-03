-- Cartelle per separare clienti e progetti quando l'archivio cresce.
CREATE TABLE folders (
  id    INTEGER PRIMARY KEY,
  name  TEXT    NOT NULL,
  color TEXT    NOT NULL DEFAULT '#0a84ff'
);

ALTER TABLE sessions ADD COLUMN folder_id INTEGER REFERENCES folders(id);

CREATE INDEX idx_sessions_folder ON sessions(folder_id);
