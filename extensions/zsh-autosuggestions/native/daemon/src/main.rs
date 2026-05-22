use clap::{Parser, Subcommand};

mod db;
mod importer;
mod protocol;
mod scorer;
mod server;
mod zsh;

#[derive(Parser)]
#[command(
    name = "zsh-autosuggestions",
    about = "智能 zsh 命令行预测补全",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "导入 shell 历史到本地数据库")]
    Import,
    #[command(about = "启动建议守护进程（Unix socket）")]
    Daemon,
    #[command(about = "查询当前建议")]
    Query {
        #[arg(long, help = "当前 zsh buffer")]
        buffer: String,
        #[arg(long, help = "当前工作目录")]
        dir: String,
        #[arg(long, help = "上一条命令")]
        prev: String,
        #[arg(long, default_value = "lines", help = "输出格式: plain|lines|json")]
        format: String,
        #[arg(long, help = "等同于 --format json")]
        json: bool,
    },
    #[command(about = "记录一条已执行的命令")]
    Record {
        #[arg(long)]
        command: String,
        #[arg(long)]
        dir: String,
        #[arg(long, name = "exit")]
        exit_code: i32,
        #[arg(long)]
        duration: i64,
        #[arg(long)]
        session: String,
        #[arg(long)]
        prev: String,
    },
    #[command(about = "输出 zsh 集成脚本")]
    Init,
    #[command(about = "检查守护进程是否运行")]
    Ping,
}

fn data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("ZSH_AS_DATA_DIR") {
        if !dir.is_empty() {
            return std::path::PathBuf::from(dir);
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("voidnix")
        .join("zsh-autosuggestions")
}

fn db_path() -> std::path::PathBuf {
    data_dir().join("zsh-autosuggestions.db")
}

fn sock_path() -> std::path::PathBuf {
    data_dir().join("sock")
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Import => {
            let path = db_path();
            let conn = match db::init_db(&path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("init db: {}", e);
                    std::process::exit(1);
                }
            };

            let zsh_history = importer::read_zsh_history().unwrap_or_default();
            let bash_history = importer::read_bash_history().unwrap_or_default();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let session_id = format!("import-{}", std::process::id());

            let commands: Vec<db::Command> = zsh_history
                .into_iter()
                .chain(bash_history)
                .filter(|rc| !rc.command.trim().is_empty())
                .map(|rc| db::Command {
                    command: rc.command,
                    directory: std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    timestamp: rc.timestamp.unwrap_or(now),
                    exit_code: 0,
                    duration_ms: rc.duration.unwrap_or(0),
                    session_id: session_id.clone(),
                })
                .collect();

            let total = commands.len();
            for chunk in commands.chunks(500) {
                if let Err(e) = db::save_import_batch(&conn, chunk) {
                    eprintln!("import error: {}", e);
                    std::process::exit(1);
                }
            }
            println!("已导入 {} 条历史命令", total);
        }

        Commands::Daemon => {
            let path = db_path();
            let sock = sock_path();
            let sock_str = sock.display().to_string();

            let server = server::Server::new(&path).unwrap_or_else(|e| {
                eprintln!("init server: {}", e);
                std::process::exit(1);
            });

            let rt = tokio::runtime::Runtime::new().unwrap();
            if let Err(e) = rt.block_on(server.serve(&sock_str)) {
                eprintln!("daemon error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Query {
            buffer,
            dir,
            prev,
            format,
            json,
        } => {
            let format = if json { "json" } else { &format };

            let resp = match query_via_socket(&sock_path(), &buffer, &dir, &prev) {
                Ok(r) => r,
                Err(_) => fallback_query(&db_path(), &buffer, &dir, &prev),
            };

            match format {
                "json" => {
                    println!(
                        "{}",
                        serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string())
                    );
                }
                "lines" => {
                    if !resp.suggestion.is_empty() {
                        println!("{}", resp.suggestion);
                        for alt in &resp.alternatives {
                            println!("{}", alt);
                        }
                    }
                }
                _ => {
                    if !resp.suggestion.is_empty() {
                        println!("{}", resp.suggestion);
                    }
                }
            }
        }

        Commands::Record {
            command,
            dir,
            exit_code,
            duration,
            session,
            prev,
        } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let cmd = db::Command {
                command,
                directory: dir,
                timestamp: now,
                exit_code,
                duration_ms: duration,
                session_id: session,
            };

            let path = db_path();
            if let Ok(conn) = db::init_db(&path) {
                db::record_command(&conn, &cmd, &prev).ok();
            }
        }

        Commands::Init => {
            let bin_path = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "zsh-autosuggestions".to_string());

            let data_dir = std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.parent()
                        .and_then(|d| d.parent())
                        .map(|d| d.display().to_string())
                })
                .unwrap_or_else(|| data_dir().display().to_string());

            let script = zsh::zsh_init()
                .replace("{{BINARY_PATH}}", &bin_path)
                .replace("{{DATA_DIR}}", &data_dir);
            println!("{}", script);
        }

        Commands::Ping => {
            match query_via_socket(&sock_path(), "", "", "") {
                Ok(_) => {
                    println!("pong");
                }
                Err(_) => {
                    eprintln!("daemon not running");
                    std::process::exit(1);
                }
            }
        }
    }
}

fn query_via_socket(
    sock: &std::path::Path,
    buffer: &str,
    dir: &str,
    prev: &str,
) -> Result<protocol::SuggestResp, String> {
    let stream =
        std::os::unix::net::UnixStream::connect(sock).map_err(|e| format!("connect: {}", e))?;

    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(150)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_millis(50)))
        .ok();

    let req = protocol::Envelope {
        msg_type: "suggest".to_string(),
        payload: serde_json::to_value(protocol::SuggestReq {
            buffer: buffer.to_string(),
            dir: dir.to_string(),
            prev: prev.to_string(),
        })
        .unwrap(),
    };

    serde_json::to_writer(&stream, &req).map_err(|e| format!("write: {}", e))?;
    let _ = std::io::Write::write(&mut &stream, b"\n");

    let resp: protocol::SuggestResp =
        serde_json::from_reader(&stream).map_err(|e| format!("read: {}", e))?;

    Ok(resp)
}

fn fallback_query(
    db_path: &std::path::Path,
    buffer: &str,
    dir: &str,
    prev: &str,
) -> protocol::SuggestResp {
    let conn = match db::init_db(db_path) {
        Ok(c) => c,
        Err(_) => return protocol::SuggestResp {
            suggestion: String::new(),
            alternatives: Vec::new(),
        },
    };

    let stats = match db::get_command_stats(&conn) {
        Ok(s) if !s.is_empty() => s,
        _ => return protocol::SuggestResp {
            suggestion: String::new(),
            alternatives: Vec::new(),
        },
    };

    let seq_counts = db::get_sequence_counts(&conn, prev).unwrap_or_default();

    let mut dir_counts: std::collections::HashMap<String, std::collections::HashMap<String, i64>> =
        std::collections::HashMap::new();
    for s in &stats {
        if let Ok(dc) = db::get_dir_counts_for_command(&conn, &s.command) {
            dir_counts.insert(s.command.clone(), dc);
        }
    }

    let now = std::time::SystemTime::now();
    let ranked = scorer::rank(&stats, buffer, dir, prev, &seq_counts, &dir_counts, now);

    if ranked.is_empty() {
        return protocol::SuggestResp {
            suggestion: String::new(),
            alternatives: Vec::new(),
        };
    }

    let alts: Vec<String> = ranked
        .iter()
        .skip(1)
        .take(4)
        .map(|r| r.command.clone())
        .collect();

    protocol::SuggestResp {
        suggestion: ranked[0].command.clone(),
        alternatives: alts,
    }
}
