//! Snippets and batches for the window (D5): the saved commands of the
//! TUI's database, and one command run on many hosts, a few at a time. Each
//! host of a batch answers on `batch-progress` when it is done.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::{anyhow, ensure, Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};
use tokio::sync::Semaphore;
use vssh_core::conn_err::ConnError;
use vssh_core::store::db::Snippet;

use crate::{Backend, Reply};

/// Global ones first, then by host and name.
#[tauri::command(async)]
pub fn snippets_list(backend: State<'_, Backend>) -> Reply<Vec<Snippet>> {
    Ok(backend.db().and_then(|db| Ok(db.snippets()?))?)
}

/// Saves a snippet as the TUI's form would: trimmed, with a name and a
/// command, for every host (`host` empty) or one that exists. Id 0 is a
/// snippet that is not saved yet; any other must be one that is.
fn save(backend: &Backend, mut snippet: Snippet) -> Result<Snippet> {
    for field in [&mut snippet.name, &mut snippet.command, &mut snippet.host] {
        *field = field.trim().to_string();
    }
    ensure!(
        !snippet.name.is_empty() && !snippet.command.is_empty(),
        "name and command are required"
    );
    if !snippet.host.is_empty() {
        backend.config_of(&snippet.host)?;
    }
    let db = backend.db()?;
    let known = db.snippets()?.iter().any(|old| old.id == snippet.id);
    ensure!(
        known || snippet.id == 0,
        "no snippet with id {}",
        snippet.id
    );
    snippet.id = db.save_snippet(&snippet)?;
    Ok(snippet)
}

#[tauri::command(async)]
pub fn snippets_create(
    name: String,
    command: String,
    host: String,
    backend: State<'_, Backend>,
) -> Reply<Snippet> {
    let id = 0;
    Ok(save(
        &backend,
        Snippet {
            id,
            host,
            name,
            command,
        },
    )?)
}

#[tauri::command(async)]
pub fn snippets_update(
    id: i64,
    name: String,
    command: String,
    host: String,
    backend: State<'_, Backend>,
) -> Reply<Snippet> {
    if id == 0 {
        // Saving it would make a new snippet: updating is no way to create.
        return Err(anyhow!("no snippet with id 0").into());
    }
    Ok(save(
        &backend,
        Snippet {
            id,
            host,
            name,
            command,
        },
    )?)
}

#[tauri::command(async)]
pub fn snippets_delete(id: i64, backend: State<'_, Backend>) -> Reply<()> {
    Ok(backend.db().and_then(|db| Ok(db.delete_snippet(id)?))?)
}

/// Puts the values in where the command says `{{name}}`. Only a plain
/// name between the braces is a parameter, so `{{.Names}}` of a Go template
/// stays as it is; a value is put in as typed and not looked into again.
fn fill(command: &str, params: &HashMap<String, String>) -> Result<String> {
    let plain = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut rest = command;
    while let Some((before, after)) = rest.split_once("{{") {
        out.push_str(before);
        let name = after.split_once("}}").map(|(name, _)| name.trim());
        match name.filter(|name| !name.is_empty() && name.chars().all(plain)) {
            Some(name) => {
                let value = params.get(name);
                out.push_str(value.with_context(|| format!("no value for {{{{{name}}}}}"))?);
                rest = after.split_once("}}").map_or("", |(_, after)| after);
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// What `batch-progress` carries: one host's answer in a batch. `output`
/// is what the command printed, `error` why it could not be run.
#[derive(Clone, Serialize)]
struct Answered {
    id: u64,
    host: String,
    output: Option<String>,
    error: Option<ConnError>,
}

fn run<R: Runtime>(
    app: AppHandle<R>,
    backend: &Backend,
    hosts: Vec<String>,
    command: &str,
    params: &HashMap<String, String>,
) -> Result<u64> {
    ensure!(!command.trim().is_empty(), "type a command to run");
    ensure!(!hosts.is_empty(), "pick at least one host");
    let command = fill(command.trim(), params)?;
    let id = backend.jobs.fetch_add(1, Ordering::Relaxed) + 1;
    let turns = Arc::new(Semaphore::new(backend.batch_max_parallel.max(1)));
    for host in hosts {
        // Nothing happens yet: the connection is made when this is awaited.
        let login = backend.login(&host);
        let connecting = login.map(|(target, password)| backend.net.connect(target, password));
        let (app, turns, command) = (app.clone(), turns.clone(), command.clone());
        tauri::async_runtime::spawn(async move {
            let answer = async {
                let _turn = turns.acquire().await;
                let run = connecting?.await?;
                run(command).await
            };
            let answer = answer.await;
            let (output, error) = match answer {
                Ok(output) => (Some(output), None),
                Err(error) => (None, Some(error)),
            };
            let answered = Answered {
                id,
                host,
                output,
                error,
            };
            let _ = app.emit("batch-progress", answered);
        });
    }
    Ok(id)
}

/// Runs one command on every host named, each over a connection of its
/// own, and returns the batch's id at once. `{{name}}` in the command is
/// replaced by `params.name`.
#[tauri::command]
pub fn snippet_run<R: Runtime>(
    app: AppHandle<R>,
    hosts: Vec<String>,
    command: String,
    params: Option<HashMap<String, String>>,
    backend: State<'_, Backend>,
) -> Reply<u64> {
    let params = params.unwrap_or_default();
    Ok(run(app, &backend, hosts, &command, &params)?)
}
