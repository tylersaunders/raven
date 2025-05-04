DROP TABLE IF EXISTS history_fts;
DROP TRIGGER IF EXISTS history_ai;
DROP TRIGGER IF EXISTS history_ad;
DROP TRIGGER IF EXISTS history_au;

CREATE VIRTUAL TABLE history_fts USING fts5(command, cwd, content='history', content_rowid='id');

-- Populate new table with existing history data
INSERT INTO history_fts SELECT command, cwd from history;

CREATE TRIGGER history_ai AFTER INSERT ON history
  BEGIN
    INSERT INTO history_fts(rowid, command, cwd)
    VALUES (new.ROWID, new.command, new.cwd);
  END;

CREATE TRIGGER history_ad AFTER DELETE ON history
  BEGIN
    INSERT INTO history_fts (history_fts, rowid, command, cwd)
    VALUES ('delete', old.id, old.command, old.cwd);
  END;

CREATE TRIGGER history_au AFTER UPDATE ON history
  BEGIN
    INSERT INTO history_fts (history_fts, rowid, command, cwd)
    VALUES ('delete', old.id, old.command, old.cwd);
    INSERT INTO history_fts(rowid, command, cwd)
    VALUES (new.ROWID, new.command, new.cwd);
  END;
