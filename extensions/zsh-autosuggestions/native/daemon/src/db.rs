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
    pub project_root: String,
    pub prev_command: String,
    pub prev_prev_command: String,
    pub prev_exit_code: i32,
}

#[derive(Debug, Clone)]
pub struct CommandStat {
    pub command: String,
    pub count: i64,
    pub last_used: i64,
    pub fail_count: i64,
    pub accept_count: i64,
    pub reject_count: i64,
    pub suggested_count: i64,
}

#[derive(Debug, Clone)]
pub struct SeqStat {
    pub count: i64,
    pub last_seen: i64,
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
            session_id TEXT NOT NULL,
            project_root TEXT NOT NULL DEFAULT '',
            prev_command TEXT NOT NULL DEFAULT '',
            prev_prev_command TEXT NOT NULL DEFAULT '',
            prev_exit_code INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_commands_command ON commands(command);
        CREATE INDEX IF NOT EXISTS idx_commands_timestamp ON commands(timestamp);
        CREATE INDEX IF NOT EXISTS idx_commands_session ON commands(session_id);

        CREATE TABLE IF NOT EXISTS command_stats (
            command TEXT PRIMARY KEY,
            count INTEGER NOT NULL DEFAULT 0,
            last_used INTEGER NOT NULL,
            fail_count INTEGER NOT NULL DEFAULT 0,
            accept_count INTEGER NOT NULL DEFAULT 0,
            reject_count INTEGER NOT NULL DEFAULT 0,
            suggested_count INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_command_stats_last_used ON command_stats(last_used);

        CREATE TABLE IF NOT EXISTS sequences (
            prev_command TEXT NOT NULL,
            next_command TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            last_seen INTEGER NOT NULL DEFAULT 0,
            UNIQUE(prev_command, next_command)
        );

        CREATE TABLE IF NOT EXISTS trigrams (
            pp TEXT NOT NULL,
            prev TEXT NOT NULL,
            next TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            last_seen INTEGER NOT NULL DEFAULT 0,
            UNIQUE(pp, prev, next)
        );

        CREATE INDEX IF NOT EXISTS idx_trigrams_pp_prev ON trigrams(pp, prev);

        CREATE TABLE IF NOT EXISTS recovery_sequences (
            prev_command TEXT NOT NULL,
            next_command TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            last_seen INTEGER NOT NULL DEFAULT 0,
            UNIQUE(prev_command, next_command)
        );",
    )?;

    // Idempotent migrations for legacy databases
    let _ = conn.execute(
        "ALTER TABLE command_stats ADD COLUMN fail_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE command_stats ADD COLUMN accept_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE command_stats ADD COLUMN reject_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE command_stats ADD COLUMN suggested_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE sequences ADD COLUMN last_seen INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE commands ADD COLUMN project_root TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE commands ADD COLUMN prev_command TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE commands ADD COLUMN prev_prev_command TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE commands ADD COLUMN prev_exit_code INTEGER NOT NULL DEFAULT 0",
        [],
    );

    // Index dependent on the project_root column — create AFTER the column migration.
    let _ = conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_commands_project ON commands(project_root);",
    );

    // Backfill last_seen from command_stats for pre-migration sequences (one-time)
    conn.execute(
        "UPDATE sequences SET last_seen = COALESCE(
            (SELECT MAX(s.last_used) FROM command_stats s WHERE s.command = sequences.next_command),
            strftime('%s', 'now')
        ) WHERE last_seen = 0",
        [],
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
            "INSERT INTO commands (command, directory, timestamp, exit_code, duration_ms, session_id, project_root, prev_command, prev_prev_command, prev_exit_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for cmd in commands {
            // 与 command_stats / sequences / trigrams 保持一致：始终写 trim 后的 command。
            // 否则 get_dir_counts_for_command / get_project_counts_for_command 用 trim 后的 key
            // 查询会查不到带空白前后缀的原始记录。
            let key = cmd.command.trim();
            if key.is_empty() {
                continue;
            }
            stmt.execute(params![
                key,
                cmd.directory,
                cmd.timestamp,
                cmd.exit_code,
                cmd.duration_ms,
                cmd.session_id,
                cmd.project_root,
                cmd.prev_command,
                cmd.prev_prev_command,
                cmd.prev_exit_code,
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
        let mut bi_stmt = tx.prepare(
            "INSERT INTO sequences (prev_command, next_command, count, last_seen) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(prev_command, next_command) DO UPDATE SET
               count = sequences.count + 1,
               last_seen = MAX(sequences.last_seen, excluded.last_seen)",
        )?;
        let mut tri_stmt = tx.prepare(
            "INSERT INTO trigrams (pp, prev, next, count, last_seen) VALUES (?1, ?2, ?3, 1, ?4)
             ON CONFLICT(pp, prev, next) DO UPDATE SET
               count = trigrams.count + 1,
               last_seen = MAX(trigrams.last_seen, excluded.last_seen)",
        )?;
        for i in 1..commands.len() {
            let prev = commands[i - 1].command.trim();
            let next = commands[i].command.trim();
            if prev.is_empty() || next.is_empty() {
                continue;
            }
            // Skip cross-session bigrams: >30min gap likely means different session
            let gap = commands[i].timestamp - commands[i - 1].timestamp;
            if gap > 1800 {
                continue;
            }
            bi_stmt.execute(params![prev, next, commands[i].timestamp])?;

            if i >= 2 {
                let pp = commands[i - 2].command.trim();
                if pp.is_empty() {
                    continue;
                }
                let pp_gap = commands[i - 1].timestamp - commands[i - 2].timestamp;
                if pp_gap > 1800 {
                    continue;
                }
                tri_stmt.execute(params![pp, prev, next, commands[i].timestamp])?;
            }
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn record_command(
    conn: &Connection,
    cmd: &Command,
) -> rusqlite::Result<()> {
    let key = cmd.command.trim();
    if key.is_empty() {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;

    tx.execute(
        "INSERT INTO commands (command, directory, timestamp, exit_code, duration_ms, session_id, project_root, prev_command, prev_prev_command, prev_exit_code)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            key,
            cmd.directory,
            cmd.timestamp,
            cmd.exit_code,
            cmd.duration_ms,
            cmd.session_id,
            cmd.project_root,
            cmd.prev_command,
            cmd.prev_prev_command,
            cmd.prev_exit_code,
        ],
    )?;

    tx.execute(
        "INSERT INTO command_stats (command, count, last_used, fail_count) VALUES (?1, 1, ?2, ?3)
         ON CONFLICT(command) DO UPDATE SET
           count = command_stats.count + 1,
           last_used = MAX(command_stats.last_used, excluded.last_used),
           fail_count = command_stats.fail_count + excluded.fail_count",
        params![key, cmd.timestamp, if cmd.exit_code != 0 { 1 } else { 0 }],
    )?;

    let prev = cmd.prev_command.trim();
    if !prev.is_empty() {
        tx.execute(
            "INSERT INTO sequences (prev_command, next_command, count, last_seen) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(prev_command, next_command) DO UPDATE SET
               count = sequences.count + 1,
               last_seen = MAX(sequences.last_seen, excluded.last_seen)",
            params![prev, key, cmd.timestamp],
        )?;

        let pp = cmd.prev_prev_command.trim();
        if !pp.is_empty() {
            tx.execute(
                "INSERT INTO trigrams (pp, prev, next, count, last_seen) VALUES (?1, ?2, ?3, 1, ?4)
                 ON CONFLICT(pp, prev, next) DO UPDATE SET
                   count = trigrams.count + 1,
                   last_seen = MAX(trigrams.last_seen, excluded.last_seen)",
                params![pp, prev, key, cmd.timestamp],
            )?;
        }

        if cmd.prev_exit_code != 0 {
            tx.execute(
                "INSERT INTO recovery_sequences (prev_command, next_command, count, last_seen) VALUES (?1, ?2, 1, ?3)
                 ON CONFLICT(prev_command, next_command) DO UPDATE SET
                   count = recovery_sequences.count + 1,
                   last_seen = MAX(recovery_sequences.last_seen, excluded.last_seen)",
                params![prev, key, cmd.timestamp],
            )?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn record_feedback(
    conn: &Connection,
    command: &str,
    kind: &str,
    timestamp: i64,
) -> rusqlite::Result<()> {
    let key = command.trim();
    if key.is_empty() {
        return Ok(());
    }

    let column = match kind {
        "accept" => "accept_count",
        "reject" => "reject_count",
        "impression" => "suggested_count",
        _ => return Ok(()),
    };

    let sql = format!(
        "INSERT INTO command_stats (command, count, last_used, {col}) VALUES (?1, 0, ?2, 1)
         ON CONFLICT(command) DO UPDATE SET {col} = command_stats.{col} + 1",
        col = column,
    );
    conn.execute(&sql, params![key, timestamp])?;
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
    // Trim rare trigrams that haven't appeared in 7d
    let trigram_cutoff = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        - 7 * 86400;
    conn.execute(
        "DELETE FROM trigrams WHERE count < 2 AND last_seen < ?1",
        params![trigram_cutoff],
    )?;
    Ok(())
}

pub fn get_command_stats(conn: &Connection) -> rusqlite::Result<Vec<CommandStat>> {
    let mut stmt = conn.prepare(
        "SELECT command, count, last_used, fail_count, accept_count, reject_count, suggested_count
         FROM command_stats ORDER BY count DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CommandStat {
            command: row.get(0)?,
            count: row.get(1)?,
            last_used: row.get(2)?,
            fail_count: row.get(3)?,
            accept_count: row.get(4)?,
            reject_count: row.get(5)?,
            suggested_count: row.get(6)?,
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
) -> rusqlite::Result<std::collections::HashMap<String, SeqStat>> {
    let mut stmt = conn.prepare(
        "SELECT next_command, count, last_seen FROM sequences WHERE prev_command = ?1 ORDER BY count DESC",
    )?;
    let rows = stmt.query_map(params![prev], |row| {
        Ok((
            row.get::<_, String>(0)?,
            SeqStat {
                count: row.get(1)?,
                last_seen: row.get(2)?,
            },
        ))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (cmd, stat) = row?;
        map.insert(cmd, stat);
    }
    Ok(map)
}

pub fn get_trigram_counts(
    conn: &Connection,
    pp: &str,
    prev: &str,
) -> rusqlite::Result<std::collections::HashMap<String, SeqStat>> {
    let mut stmt = conn.prepare(
        "SELECT next, count, last_seen FROM trigrams WHERE pp = ?1 AND prev = ?2 ORDER BY count DESC",
    )?;
    let rows = stmt.query_map(params![pp, prev], |row| {
        Ok((
            row.get::<_, String>(0)?,
            SeqStat {
                count: row.get(1)?,
                last_seen: row.get(2)?,
            },
        ))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (cmd, stat) = row?;
        map.insert(cmd, stat);
    }
    Ok(map)
}

pub fn get_recovery_counts(
    conn: &Connection,
    prev: &str,
) -> rusqlite::Result<std::collections::HashMap<String, SeqStat>> {
    let mut stmt = conn.prepare(
        "SELECT next_command, count, last_seen FROM recovery_sequences WHERE prev_command = ?1 ORDER BY count DESC",
    )?;
    let rows = stmt.query_map(params![prev], |row| {
        Ok((
            row.get::<_, String>(0)?,
            SeqStat {
                count: row.get(1)?,
                last_seen: row.get(2)?,
            },
        ))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (cmd, stat) = row?;
        map.insert(cmd, stat);
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

pub fn get_project_counts_for_command(
    conn: &Connection,
    cmd: &str,
) -> rusqlite::Result<std::collections::HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT project_root, COUNT(*) as n FROM commands
         WHERE command = ?1 AND project_root != '' GROUP BY project_root",
    )?;
    let rows = stmt.query_map(params![cmd], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (root, count) = row?;
        map.insert(root, count);
    }
    Ok(map)
}

/// 一次性加载所有命令的目录计数，避免启动时按 stat 逐条查询（N+1）。
pub fn get_all_dir_counts(
    conn: &Connection,
) -> rusqlite::Result<std::collections::HashMap<String, std::collections::HashMap<String, i64>>> {
    let mut stmt = conn.prepare(
        "SELECT command, directory, COUNT(*) as n FROM commands
         GROUP BY command, directory",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;

    let mut map: std::collections::HashMap<String, std::collections::HashMap<String, i64>> =
        std::collections::HashMap::new();
    for row in rows {
        let (cmd, dir, n) = row?;
        map.entry(cmd).or_default().insert(dir, n);
    }
    Ok(map)
}

/// 一次性加载所有命令的项目根计数，避免启动时按 stat 逐条查询（N+1）。
pub fn get_all_project_counts(
    conn: &Connection,
) -> rusqlite::Result<std::collections::HashMap<String, std::collections::HashMap<String, i64>>> {
    let mut stmt = conn.prepare(
        "SELECT command, project_root, COUNT(*) as n FROM commands
         WHERE project_root != '' GROUP BY command, project_root",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;

    let mut map: std::collections::HashMap<String, std::collections::HashMap<String, i64>> =
        std::collections::HashMap::new();
    for row in rows {
        let (cmd, root, n) = row?;
        map.entry(cmd).or_default().insert(root, n);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn temp_db() -> Connection {
        let n = TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "zsh-as-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            n,
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        init_db(&path).unwrap()
    }

    fn make_cmd(command: &str, ts: i64, prev: &str, pp: &str, prev_exit: i32) -> Command {
        Command {
            command: command.to_string(),
            directory: "/tmp".to_string(),
            timestamp: ts,
            exit_code: 0,
            duration_ms: 10,
            session_id: "s".to_string(),
            project_root: "/tmp/proj".to_string(),
            prev_command: prev.to_string(),
            prev_prev_command: pp.to_string(),
            prev_exit_code: prev_exit,
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("zsh-as-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mig.db");

        // Simulate legacy schema (pre-new columns)
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE commands (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    command TEXT NOT NULL,
                    directory TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    exit_code INTEGER NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    session_id TEXT NOT NULL
                );
                CREATE TABLE command_stats (
                    command TEXT PRIMARY KEY,
                    count INTEGER NOT NULL DEFAULT 0,
                    last_used INTEGER NOT NULL
                );
                CREATE TABLE sequences (
                    prev_command TEXT NOT NULL,
                    next_command TEXT NOT NULL,
                    count INTEGER NOT NULL DEFAULT 0,
                    UNIQUE(prev_command, next_command)
                );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO command_stats (command, count, last_used) VALUES ('git status', 5, 0)",
                [],
            )
            .unwrap();
        }

        // Run new init_db twice
        init_db(&path).unwrap();
        let conn = init_db(&path).unwrap();
        let stats = get_command_stats(&conn).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].command, "git status");
        assert_eq!(stats[0].count, 5);
        assert_eq!(stats[0].accept_count, 0);
        assert_eq!(stats[0].suggested_count, 0);
    }

    #[test]
    fn trigram_written_only_with_pp() {
        let conn = temp_db();
        record_command(&conn, &make_cmd("git push", 1000, "git commit", "git add", 0)).unwrap();
        let tri = get_trigram_counts(&conn, "git add", "git commit").unwrap();
        assert_eq!(tri.get("git push").map(|s| s.count), Some(1));

        record_command(&conn, &make_cmd("ls", 1001, "cd /", "", 0)).unwrap();
        let tri = get_trigram_counts(&conn, "", "cd /").unwrap();
        assert!(tri.is_empty());
    }

    #[test]
    fn recovery_written_only_on_fail() {
        let conn = temp_db();
        record_command(&conn, &make_cmd("cargo fix", 1000, "cargo build", "", 1)).unwrap();
        let rec = get_recovery_counts(&conn, "cargo build").unwrap();
        assert_eq!(rec.get("cargo fix").map(|s| s.count), Some(1));

        record_command(&conn, &make_cmd("cargo run", 1001, "cargo build", "", 0)).unwrap();
        let rec = get_recovery_counts(&conn, "cargo build").unwrap();
        assert!(rec.get("cargo run").is_none());
    }

    #[test]
    fn feedback_increments_counters() {
        let conn = temp_db();
        record_command(&conn, &make_cmd("git status", 1000, "", "", 0)).unwrap();
        record_feedback(&conn, "git status", "accept", 1001).unwrap();
        record_feedback(&conn, "git status", "accept", 1002).unwrap();
        record_feedback(&conn, "git status", "reject", 1003).unwrap();
        record_feedback(&conn, "git status", "impression", 1004).unwrap();
        record_feedback(&conn, "git status", "impression", 1005).unwrap();

        let stats = get_command_stats(&conn).unwrap();
        let s = stats.iter().find(|s| s.command == "git status").unwrap();
        assert_eq!(s.accept_count, 2);
        assert_eq!(s.reject_count, 1);
        assert_eq!(s.suggested_count, 2);
    }

    #[test]
    fn feedback_on_unknown_command_creates_row() {
        let conn = temp_db();
        record_feedback(&conn, "rare cmd", "impression", 1000).unwrap();
        let stats = get_command_stats(&conn).unwrap();
        let s = stats.iter().find(|s| s.command == "rare cmd").unwrap();
        assert_eq!(s.suggested_count, 1);
        assert_eq!(s.count, 0);
    }

    #[test]
    fn project_counts_aggregate() {
        let conn = temp_db();
        let mut cmd = make_cmd("git status", 1000, "", "", 0);
        cmd.project_root = "/a".to_string();
        record_command(&conn, &cmd).unwrap();
        cmd.timestamp = 1001;
        cmd.project_root = "/a".to_string();
        record_command(&conn, &cmd).unwrap();
        cmd.timestamp = 1002;
        cmd.project_root = "/b".to_string();
        record_command(&conn, &cmd).unwrap();

        let m = get_project_counts_for_command(&conn, "git status").unwrap();
        assert_eq!(m.get("/a").copied(), Some(2));
        assert_eq!(m.get("/b").copied(), Some(1));
    }
}
