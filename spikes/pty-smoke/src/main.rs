use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::BTreeSet;
use std::error::Error;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_POSITION_REPLY: &[u8] = b"\x1b[1;1R";
const PREVIEW_BYTES: usize = 800;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: pty-smoke <seconds> <program> [args...]".into());
    }
    let run_for = Duration::from_secs(args[0].parse()?);

    let pair = native_pty_system().openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut command = CommandBuilder::new(&args[1]);
    command.args(&args[2..]);
    command.cwd(std::env::current_dir()?);

    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    println!("spawned pid: {:?}", child.process_id());

    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;

    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        while let Ok(read) = reader.read(&mut buffer) {
            if read == 0 || sender.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
    });

    let started = Instant::now();
    let mut output = Vec::new();
    let mut first_byte_after = None;
    let mut exit_status = None;

    while started.elapsed() < run_for {
        if let Ok(chunk) = receiver.recv_timeout(Duration::from_millis(100)) {
            first_byte_after.get_or_insert(started.elapsed());
            // ConPTY started with INHERIT_CURSOR stalls until the host answers this query.
            if contains(&chunk, CURSOR_POSITION_QUERY) {
                writer.write_all(CURSOR_POSITION_REPLY)?;
                writer.flush()?;
            }
            output.extend_from_slice(&chunk);
        }
        if let Some(status) = child.try_wait()? {
            exit_status = Some(status);
            while let Ok(chunk) = receiver.recv_timeout(Duration::from_millis(300)) {
                output.extend_from_slice(&chunk);
            }
            break;
        }
    }

    println!("exit status before kill: {exit_status:?}");
    if exit_status.is_none() {
        println!("kill() returned: {:?}", child.kill());
        thread::sleep(Duration::from_millis(500));
        println!("try_wait() after kill: {:?}", child.try_wait());
    }

    println!("first output after: {first_byte_after:?}");
    println!("total bytes: {}", output.len());
    println!("OSC codes seen: {:?}", osc_codes(&output));
    println!("private modes set: {:?}", private_modes(&output));
    println!(
        "preview: {}",
        output
            .iter()
            .take(PREVIEW_BYTES)
            .flat_map(|byte| std::ascii::escape_default(*byte))
            .map(char::from)
            .collect::<String>()
    );
    Ok(())
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn osc_codes(bytes: &[u8]) -> BTreeSet<String> {
    numbers_after(bytes, b"\x1b]", b";")
}

fn private_modes(bytes: &[u8]) -> BTreeSet<String> {
    numbers_after(bytes, b"\x1b[?", b"h")
}

fn numbers_after(bytes: &[u8], prefix: &[u8], terminator: &[u8]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut index = 0;
    while index + prefix.len() <= bytes.len() {
        if &bytes[index..index + prefix.len()] == prefix {
            let digits_start = index + prefix.len();
            let digits_end = bytes[digits_start..]
                .iter()
                .position(|byte| !byte.is_ascii_digit())
                .map_or(bytes.len(), |offset| digits_start + offset);
            if digits_end > digits_start && bytes[digits_end..].starts_with(terminator) {
                found.insert(String::from_utf8_lossy(&bytes[digits_start..digits_end]).into_owned());
            }
            index = digits_end.max(index + 1);
        } else {
            index += 1;
        }
    }
    found
}
