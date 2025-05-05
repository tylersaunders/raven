-- Rename history temporarily to avoid data loss.
ALTER TABLE history rename to history_old;

-- Add a unique constraint that prevents duplicate commands
CREATE TABLE IF NOT EXISTS history (
  id INTEGER PRIMARY KEY ASC,
  timestamp INTEGER,
  command TEXT NOT NULL,
  cwd TEXT,
  exit_code INT NOT NULL,
  CONSTRAINT history_unique UNIQUE(command, cwd, exit_code) ON CONFLICT REPLACE
);

-- Migrate history_old back into history.
INSERT INTO history SELECT * FROM history_old;
DROP TABLE IF EXISTS history_old;
