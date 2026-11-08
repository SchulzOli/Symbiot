//! `symbiot-import`: loads an export as raw data into an adapter's own store (E14, M7).
//!
//! ```text
//! symbiot-import <telegram|whatsapp|mbox|git> <pfad> [--daten ORDNER] [--chat NAME]
//!                [--utc-offset MINUTEN] [--abgleich URL]
//! ```
//!
//! The adapter is a device of its own (E7): its key and chain live in `--daten`
//! (default `symbiot-import/<quelle>`). To belong to the person, it is certified once with the
//! 24 words from the environment variable `SYMBIOT_WORDS` (never as an argument, so they do
//! not end up in the shell history). With `--abgleich` it sends its chain to the home server.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;

use symbiot_import::{FileData, Options, Source};
use symbiot_store::Store;
use symbiot_sync::Remote;

const USAGE: &str = "Aufruf: symbiot-import <telegram|whatsapp|mbox|git> <pfad> \
[--daten ORDNER] [--chat NAME] [--utc-offset MINUTEN] [--abgleich URL]

Die 24 Wörter des Personen-Schlüssels kommen beim ersten Mal aus SYMBIOT_WORDS.";

struct Args {
    source: Source,
    path: PathBuf,
    data: PathBuf,
    opts: Options,
    sync: Option<String>,
}

fn args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let source = it.next().ok_or(USAGE)?;
    let source =
        Source::parse(&source).ok_or_else(|| format!("unbekannte Quelle „{source}“\n\n{USAGE}"))?;
    let path = PathBuf::from(it.next().ok_or(USAGE)?);
    let mut data = None;
    let mut opts = Options::default();
    let mut sync = None;
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| format!("{flag} braucht einen Wert"))
        };
        match flag.as_str() {
            "--daten" => data = Some(PathBuf::from(value()?)),
            "--chat" => opts.chat = Some(value()?),
            "--utc-offset" => {
                opts.utc_offset_min = value()?
                    .parse()
                    .map_err(|_| "--utc-offset in Minuten, z. B. 60".to_string())?
            }
            "--abgleich" => sync = Some(value()?),
            other => return Err(format!("unbekannte Option {other}\n\n{USAGE}")),
        }
    }
    let data = data.unwrap_or_else(|| PathBuf::from("symbiot-import").join(source.name()));
    Ok(Args {
        source,
        path,
        data,
        opts,
        sync,
    })
}

fn run(a: Args) -> Result<(), String> {
    let import = a
        .source
        .read(&a.path, &a.opts)
        .map_err(|e| format!("{}: {e}", a.path.display()))?;
    println!(
        "{} Belege und {} Dateien gelesen aus {}",
        import.items.len(),
        import.files.len(),
        a.path.display()
    );

    let mut store = Store::open_dir(&a.data).map_err(|e| e.to_string())?;
    if store.person().map_err(|e| e.to_string())?.is_none() {
        match std::env::var("SYMBIOT_WORDS") {
            Ok(words) => {
                store
                    .certify_self(&words, &format!("{}-import", a.source.name()))
                    .map_err(|e| e.to_string())?;
                println!("Adapter als Gerät der Person bestätigt");
            }
            Err(_) if a.sync.is_some() => {
                return Err("Für den Abgleich muss der Adapter bestätigt sein: \
                            SYMBIOT_WORDS setzen (24 Wörter)"
                    .into());
            }
            Err(_) => println!("Hinweis: Adapter nicht bestätigt, Daten bleiben hier"),
        }
    }

    // Files first: once the raw data is in the chain, its files should be here.
    for f in &import.files {
        let hash = match &f.data {
            FileData::Path(p) => store.put_file_from(p).map(|(h, _)| h),
            FileData::Bytes(b) => store.put_file(b),
        }
        .map_err(|e| e.to_string())?;
        if hash != f.hash {
            return Err("Eine Datei hat sich während des Imports geändert".into());
        }
    }
    let r = store
        .record_evidence(import.items)
        .map_err(|e| e.to_string())?;
    println!(
        "neu: {}, schon bekannt: {}, Bündel: {} (Daten in {})",
        r.added,
        r.known,
        r.entries,
        a.data.display()
    );

    if let Some(url) = a.sync {
        // Push only: an adapter sends its chain and does not need the others'.
        let store = Mutex::new(store);
        let remote = Remote::new(&url);
        let me = store.lock().map_err(|_| "Speicher gesperrt")?.device();
        let heads = remote.heads(&store).map_err(|e| e.to_string())?;
        let known = heads.iter().find(|h| h.author == me).map(|h| h.seq);
        let mine = store
            .lock()
            .map_err(|_| "Speicher gesperrt")?
            .chain_since(&me, known)
            .map_err(|e| e.to_string())?;
        let sent = remote.push_all(&store, &mine).map_err(|e| e.to_string())?;
        let files = remote.send_files(&store).map_err(|e| e.to_string())?;
        println!("an {url} gesendet: {sent} Einträge, {files} Dateien");
    }
    Ok(())
}

fn main() -> ExitCode {
    match args().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
