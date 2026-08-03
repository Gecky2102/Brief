-- Righe escluse dal report ma conservate nella trascrizione: serve per i
-- passaggi personali che non devono finire in un documento condiviso.
ALTER TABLE segments ADD COLUMN excluded INTEGER NOT NULL DEFAULT 0;
