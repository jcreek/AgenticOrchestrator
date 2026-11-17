use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;

const READ_BUFFER_BYTES: usize = 64 * 1024;
const COALESCE_WINDOW: Duration = Duration::from_millis(8);
const COALESCE_MAX_BYTES: usize = 64 * 1024;
const FLOOD_END_MARKER: &str = "AO_FLOOD_END";

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
struct Sessions {
    next_id: AtomicU32,
    open: Arc<Mutex<HashMap<u32, Session>>>,
}

fn log_path() -> PathBuf {
    std::env::temp_dir().join("ao-spike.log")
}

fn text_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
fn pty_open(
    sessions: State<Sessions>,
    program: String,
    args: Vec<String>,
    cols: u16,
    rows: u16,
    on_data: Channel<InvokeResponseBody>,
    on_exit: Channel<u32>,
) -> Result<u32, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(text_error)?;

    let mut command = CommandBuilder::new(program);
    command.args(args);
    command.cwd(std::env::temp_dir());

    let mut child = pair.slave.spawn_command(command).map_err(text_error)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(text_error)?;
    let writer = pair.master.take_writer().map_err(text_error)?;
    let id = sessions.next_id.fetch_add(1, Ordering::Relaxed);

    sessions.open.lock().map_err(text_error)?.insert(
        id,
        Session {
            master: pair.master,
            writer,
            killer: child.clone_killer(),
        },
    );

    let (chunk_sender, chunk_receiver) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buffer = vec![0u8; READ_BUFFER_BYTES];
        while let Ok(read) = reader.read(&mut buffer) {
            if read == 0 || chunk_sender.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
    });
    thread::spawn(move || {
        let mut last_sent: Option<Instant> = None;
        while let Ok(mut batch) = chunk_receiver.recv() {
            // After a quiet spell the chunk goes out at once so typing echo is not delayed.
            let batch_until = last_sent
                .map(|sent| sent + COALESCE_WINDOW)
                .filter(|deadline| *deadline > Instant::now());
            if let Some(deadline) = batch_until {
                while batch.len() < COALESCE_MAX_BYTES {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    match chunk_receiver.recv_timeout(remaining) {
                        Ok(more) => batch.extend_from_slice(&more),
                        Err(_) => break,
                    }
                }
            }
            if on_data.send(InvokeResponseBody::Raw(batch)).is_err() {
                break;
            }
            last_sent = Some(Instant::now());
        }
    });

    let open = Arc::clone(&sessions.open);
    thread::spawn(move || {
        let exit_code = child.wait().map_or(u32::MAX, |status| status.exit_code());
        if let Ok(mut open) = open.lock() {
            open.remove(&id);
        }
        let _ = on_exit.send(exit_code);
    });

    Ok(id)
}

#[tauri::command]
fn pty_write(sessions: State<Sessions>, id: u32, data: String) -> Result<(), String> {
    let mut open = sessions.open.lock().map_err(text_error)?;
    let session = open.get_mut(&id).ok_or("no such session")?;
    session.writer.write_all(data.as_bytes()).map_err(text_error)?;
    session.writer.flush().map_err(text_error)
}

#[tauri::command]
fn pty_resize(sessions: State<Sessions>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let open = sessions.open.lock().map_err(text_error)?;
    let session = open.get(&id).ok_or("no such session")?;
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(text_error)
}

#[tauri::command]
fn pty_kill(sessions: State<Sessions>, id: u32) -> Result<(), String> {
    let mut open = sessions.open.lock().map_err(text_error)?;
    let session = open.get_mut(&id).ok_or("no such session")?;
    session.killer.kill().map_err(text_error)
}

#[tauri::command]
fn make_flood_file(megabytes: usize) -> Result<String, String> {
    let path = std::env::temp_dir().join("ao-flood.txt");
    let mut file = BufWriter::new(File::create(&path).map_err(text_error)?);
    let line = format!("{}\r\n", "the quick brown fox jumps over the lazy dog ".repeat(2));
    let lines = megabytes * 1024 * 1024 / line.len();
    for _ in 0..lines {
        file.write_all(line.as_bytes()).map_err(text_error)?;
    }
    writeln!(file, "{FLOOD_END_MARKER}").map_err(text_error)?;
    file.flush().map_err(text_error)?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn selftest_requested() -> bool {
    std::env::var("AO_SELFTEST").is_ok_and(|value| value == "1")
}

#[tauri::command]
fn spike_log(line: String) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
        .map_err(text_error)?;
    writeln!(file, "{line}").map_err(text_error)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = std::fs::remove_file(log_path());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Sessions::default())
        .invoke_handler(tauri::generate_handler![
            pty_open,
            pty_write,
            pty_resize,
            pty_kill,
            make_flood_file,
            selftest_requested,
            spike_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
