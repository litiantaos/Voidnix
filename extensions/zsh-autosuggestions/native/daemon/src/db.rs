use rusqlite::{params, Connection};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Command {
    pub command: String,
    pub directory: String,
    pub timestamp: i64,
    pub exit_code: i32,
    pub duration_ms: i64,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct CommandStat {
    pub command: String,
    pub count: i64,
    pub last_used: i64,
}

#[allow(dead_code)]
pub struct SequencePair {
    pub prev_command: String,
    pub next_command: String,
    pub count: i64,
}

pub fn init_db(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let conn = Connection::open(path)?;

    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS commands (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            command TEXT NOT NULL,
            directory TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            exit_code INTEGER NOT NULL,
            duration_ms INTEGER NOT NULL,
            session_id TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_commands_command ON commands(command);
        CREATE INDEX IF NOT EXISTS idx_commands_timestamp ON commands(timestamp);
        CREATE INDEX IF NOT EXISTS idx_commands_session ON commands(session_id);

        CREATE TABLE IF NOT EXISTS command_stats (
            command TEXT PRIMARY KEY,
            count INTEGER NOT NULL DEFAULT 0,
            last_used INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_command_stats_last_used ON command_stats(last_used);

        CREATE TABLE IF NOT EXISTS sequences (
            prev_command TEXT NOT NULL,
            next_command TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            UNIQUE(prev_command, next_command)
        );",
    )?;

    Ok(conn)
}

pub fn save_import_batch(
    conn: &Connection,
    commands: &[Command],
) -> rusqlite::Result<()> {
    if commands.is_empty() {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;

    {
        let mut stmt = tx.prepare(
            "INSERT INTO commands (command, directory, timestamp, exit_code, duration_ms, session_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for cmd in commands {
            stmt.execute(params![
                cmd.command,
                cmd.directory,
                cmd.timestamp,
                cmd.exit_code,
                cmd.duration_ms,
                cmd.session_id,
            ])?;
        }
    }

    {
        let mut stmt = tx.prepare(
            "INSERT INTO command_stats (command, count, last_used) VALUES (?1, 1, ?2)
             ON CONFLICT(command) DO UPDATE SET
               count = command_stats.count + 1,
               last_used = MAX(command_stats.last_used, excluded.last_used)",
        )?;
        for cmd in commands {
            if cmd.command.trim().is_empty() {
                continue;
            }
            stmt.execute(params![cmd.command.trim(), cmd.timestamp])?;
        }
    }

    {
        let mut stmt = tx.prepare(
            "INSERT INTO sequences (prev_command, next_command, count) VALUES (?1, ?2, 1)
             ON CONFLICT(prev_command, next_command) DO UPDATE SET
               count = sequences.count + 1",
        )?;
        for i in 1..commands.len() {
            let prev = commands[i - 1].command.trim();
            let next = commands[i].command.trim();
            if prev.is_empty() || next.is_empty() {
                continue;
            }
            stmt.execute(params![prev, next])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn record_command(
    conn: &Connection,
    cmd: &Command,
    prev_command: &str,
) -> rusqlite::Result<()> {
    let key = cmd.command.trim();
    if key.is_empty() {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;

    tx.execute(
        "INSERT INTO commands (command, directory, timestamp, exit_code, duration_ms, session_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            cmd.command,
            cmd.directory,
            cmd.timestamp,
            cmd.exit_code,
            cmd.duration_ms,
            cmd.session_id,
        ],
    )?;

    tx.execute(
        "INSERT INTO command_stats (command, count, last_used) VALUES (?1, 1, ?2)
         ON CONFLICT(command) DO UPDATE SET
           count = command_stats.count + 1,
           last_used = MAX(command_stats.last_used, excluded.last_used)",
        params![key, cmd.timestamp],
    )?;

    let prev = prev_command.trim();
    if !prev.is_empty() {
        tx.execute(
            "INSERT INTO sequences (prev_command, next_command, count) VALUES (?1, ?2, 1)
             ON CONFLICT(prev_command, next_command) DO UPDATE SET
               count = sequences.count + 1",
            params![prev, key],
        )?;
    }

    tx.commit()?;
    Ok(())
}

pub fn cleanup_old_commands(conn: &Connection, retention_days: i64) -> rusqlite::Result<()> {
    if retention_days <= 0 {
        return Ok(());
    }
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        - retention_days * 86400;
    conn.execute("DELETE FROM commands WHERE timestamp < ?1", params![cutoff])?;
    Ok(())
}

pub fn get_command_stats(conn: &Connection) -> rusqlite::Result<Vec<CommandStat>> {
    let mut stmt = conn.prepare(
        "SELECT command, count, last_used FROM command_stats ORDER BY count DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CommandStat {
            command: row.get(0)?,
            count: row.get(1)?,
            last_used: row.get(2)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_sequence_counts(
    conn: &Connection,
    prev: &str,
) -> rusqlite::Result<std::collections::HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT next_command, count FROM sequences WHERE prev_command = ?1 ORDER BY count DESC",
    )?;
    let rows = stmt.query_map(params![prev], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (cmd, count) = row?;
        map.insert(cmd, count);
    }
    Ok(map)
}

pub fn get_dir_counts_for_command(
    conn: &Connection,
    cmd: &str,
) -> rusqlite::Result<std::collections::HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT directory, COUNT(*) as n FROM commands WHERE command = ?1 GROUP BY directory",
    )?;
    let rows = stmt.query_map(params![cmd], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (dir, count) = row?;
        map.insert(dir, count);
    }
    Ok(map)
}
