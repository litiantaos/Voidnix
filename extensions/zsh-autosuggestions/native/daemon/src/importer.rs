use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;

static ZSH_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^:\s*(\d+):(\d+);(.*)$").unwrap());

#[derive(Debug, Clone)]
pub struct RawCommand {
    pub command: String,
    pub timestamp: Option<i64>,
    pub duration: Option<i64>,
}

pub fn read_zsh_history() -> Result<Vec<RawCommand>, String> {
    let home = dirs::home_dir().ok_or("cannot get home directory")?;
    let history_path = home.join(".zsh_history");
    if !history_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read(&history_path)
        .map_err(|e| format!("cannot read {:?}: {}", history_path, e))?;
    let content = String::from_utf8_lossy(&content).to_string();
    let parsed = parse_zsh_history(&content);
    Ok(parsed)
}

pub fn read_bash_history() -> Result<Vec<RawCommand>, String> {
    let home = dirs::home_dir().ok_or("cannot get home directory")?;
    let history_path = home.join(".bash_history");

    if !history_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read(&history_path)
        .map_err(|e| format!("cannot read {:?}: {}", history_path, e))?;
    let content = String::from_utf8_lossy(&content).to_string();

    Ok(parse_bash_history(&content))
}

#[allow(dead_code)]
pub fn get_history_file_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let zsh = home.join(".zsh_history");
    if zsh.exists() {
        return Some(zsh);
    }
    let bash = home.join(".bash_history");
    if bash.exists() {
        return Some(bash);
    }
    None
}

fn parse_zsh_history(content: &str) -> Vec<RawCommand> {
    let mut commands = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(caps) = ZSH_LINE_RE.captures(line) {
            let ts = caps.get(1).and_then(|m| m.as_str().parse::<i64>().ok());
            let dur = caps.get(2).and_then(|m| m.as_str().parse::<i64>().ok());
            let cmd = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();

            commands.push(RawCommand {
                command: cmd,
                timestamp: ts,
                duration: dur,
            });
        } else {
            commands.push(RawCommand {
                command: line.to_string(),
                timestamp: None,
                duration: None,
            });
        }
    }

    commands
}

fn parse_bash_history(content: &str) -> Vec<RawCommand> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| RawCommand {
            command: l.trim().to_string(),
            timestamp: None,
            duration: None,
        })
        .collect()
}
