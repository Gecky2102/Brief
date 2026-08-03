-- Impronte semantiche dei segmenti: calcolarle è lento, si conservano.
CREATE TABLE embeddings (
  segment_id INTEGER PRIMARY KEY REFERENCES segments(id) ON DELETE CASCADE,
  vector     TEXT NOT NULL
);
