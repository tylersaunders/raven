CREATE TABLE IF NOT EXISTS history (
  id INTEGER PRIMARY KEY ASC,
  timestamp INTEGER,
  command TEXT NOT NULL,
  cwd TEXT,
  exit_code INT NOT NULL
)
