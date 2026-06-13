use clap::{Parser, Subcommand};

mod db;
mod importer;
mod protocol;
mod recall;
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
        #[arg(long = "prev-prev", default_value = "", help = "再上一条命令")]
        prev_prev: String,
        #[arg(long = "prev-exit", default_value = "0", help = "上一条命令的退出码")]
        prev_exit: i32,
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
        // 标志名为 `--exit` 以匹配 zsh 端 init.zsh 的调用约定。
        // 曾用 `#[arg(long, name = "exit")]`，clap v4 默认不做前缀匹配，
        // zsh 发 `--exit` 一直被静默拒绝（错误被 `>/dev/null 2>&1` 吞掉），
        // 导致命令历史从未被记录。
        #[arg(long = "exit", value_name = "exit")]
        exit_code: i32,
        #[arg(long)]
        duration: i64,
        #[arg(long)]
        session: String,
        #[arg(long)]
        prev: String,
        #[arg(long = "prev-prev", default_value = "")]
        prev_prev: String,
        #[arg(long = "prev-exit", default_value = "0")]
        prev_exit: i32,
    },
    #[command(about = "反馈：accept / reject / impression")]
    Feedback {
        #[arg(long)]
        command: String,
        #[arg(long)]
        kind: String,
        #[arg(long, default_value = "")]
        session: String,
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
    // UNIX domain socket sun_path is capped at 104 bytes on macOS. data_dir under
    // `~/Library/Application Support/<bundle_id>/extensions/zsh-autosuggestions/`
    // is already close to the limit and overflows in dev mode (bundle_id+".dev").
    // Use /tmp with the effective uid as the namespace key.
    let uid = unsafe { libc::geteuid() };
    let suffix = std::env::var("ZSH_AS_SOCK_SUFFIX").unwrap_or_default();
    let name = if suffix.is_empty() {
        format!("voidnix-zsh-as-{}.sock", uid)
    } else {
        format!("voidnix-zsh-as-{}-{}.sock", uid, suffix)
    };
    std::path::PathBuf::from("/tmp").join(name)
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
                    directory: String::new(),
                    timestamp: rc.timestamp.unwrap_or(now),
                    exit_code: 0,
                    duration_ms: rc.duration.unwrap_or(0),
                    session_id: session_id.clone(),
                    project_root: String::new(),
                    prev_command: String::new(),
                    prev_prev_command: String::new(),
                    prev_exit_code: 0,
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
            // 不再 setsid/double-fork：daemon 现由 launchd 托管，launchd 在新 session
            // 中 spawn 进程，没有控制 TTY，与任何终端解耦。zsh 端也不再 spawn。
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
            prev_prev,
            prev_exit,
            format,
            json,
        } => {
            let format = if json { "json" } else { &format };

            let req = protocol::SuggestReq {
                buffer: buffer.clone(),
                dir: dir.clone(),
                prev: prev.clone(),
                prev_prev: prev_prev.clone(),
                prev_exit,
            };
            let resp = match query_via_socket(
                &sock_path(),
                &buffer,
                &dir,
                &prev,
                &prev_prev,
                prev_exit,
            ) {
                Ok(r) => r,
                Err(_) => fallback_query(&db_path(), &req),
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
            prev_prev,
            prev_exit,
        } => {
            let req = protocol::RecordReq {
                command,
                dir,
                exit_code,
                duration,
                session,
                prev,
                prev_prev,
                prev_exit,
            };
            // Daemon 不可用时回落到直接 DB 写入。
            // 数据不会丢：daemon 重启后从 DB 全量重载即可看到这些记录。
            if record_via_socket(&sock_path(), &req).is_err() {
                let path = db_path();
                if let Ok(conn) = db::init_db(&path) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let cmd = db::Command {
                        command: req.command,
                        directory: req.dir,
                        timestamp: now,
                        exit_code: req.exit_code,
                        duration_ms: req.duration,
                        session_id: req.session,
                        project_root: String::new(),
                        prev_command: req.prev,
                        prev_prev_command: req.prev_prev,
                        prev_exit_code: req.prev_exit,
                    };
                    let _ = db::record_command(&conn, &cmd);
                }
            }
        }

        Commands::Feedback {
            command,
            kind,
            session,
        } => {
            let req = protocol::FeedbackReq {
                command,
                kind,
                session,
            };
            if feedback_via_socket(&sock_path(), &req).is_err() {
                let path = db_path();
                if let Ok(conn) = db::init_db(&path) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let _ = db::record_feedback(&conn, &req.command, &req.kind, now);
                }
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
            match query_via_socket(&sock_path(), "", "", "", "", 0) {
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
    prev_prev: &str,
    prev_exit: i32,
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
            prev_prev: prev_prev.to_string(),
            prev_exit,
        })
        .unwrap(),
    };

    serde_json::to_writer(&stream, &req).map_err(|e| format!("write: {}", e))?;
    let _ = std::io::Write::write(&mut &stream, b"\n");

    let resp: protocol::SuggestResp =
        serde_json::from_reader(&stream).map_err(|e| format!("read: {}", e))?;

    Ok(resp)
}

fn record_via_socket(
    sock: &std::path::Path,
    req: &protocol::RecordReq,
) -> Result<(), String> {
    let stream =
        std::os::unix::net::UnixStream::connect(sock).map_err(|e| format!("connect: {}", e))?;

    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(100)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_millis(50)))
        .ok();

    let env = protocol::Envelope {
        msg_type: "record".to_string(),
        payload: serde_json::to_value(req).unwrap(),
    };

    serde_json::to_writer(&stream, &env).map_err(|e| format!("write: {}", e))?;
    let _ = std::io::Write::write(&mut &stream, b"\n");

    let mut buf = [0u8; 64];
    let _ = std::io::Read::read(&mut &stream, &mut buf);

    Ok(())
}

fn feedback_via_socket(
    sock: &std::path::Path,
    req: &protocol::FeedbackReq,
) -> Result<(), String> {
    let stream =
        std::os::unix::net::UnixStream::connect(sock).map_err(|e| format!("connect: {}", e))?;

    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(100)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_millis(50)))
        .ok();

    let env = protocol::Envelope {
        msg_type: "feedback".to_string(),
        payload: serde_json::to_value(req).unwrap(),
    };

    serde_json::to_writer(&stream, &env).map_err(|e| format!("write: {}", e))?;
    let _ = std::io::Write::write(&mut &stream, b"\n");

    let mut buf = [0u8; 64];
    let _ = std::io::Read::read(&mut &stream, &mut buf);

    Ok(())
}

/// Daemon 不可用时的降级查询：直接读 DB 重建状态并跑完整排序。
/// 性能远不如常驻 daemon（无缓存），但保证功能可用。
fn fallback_query(db_path: &std::path::Path, req: &protocol::SuggestReq) -> protocol::SuggestResp {
    server::Server::query_once(db_path, req)
}
