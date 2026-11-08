//! #30: the raw data (E14) as a timeline, newest first, with full-text search.
//!
//! Raw data is not the graph: nothing here can be edited. It is what the adapters loaded,
//! grouped by day, so the person can see and find it.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::icons::{Icon, Svg};
use symbiot_core::evidence::role;
use symbiot_core::{Evidence, FileRef};
use wasm_bindgen::JsValue;

use crate::api::{self, EvidenceFilter, StoredEvidence};

/// Items per page.
const PAGE: usize = 50;
/// Longer texts are cut here; the rest is one click away.
const PREVIEW: usize = 400;

/// The person whose timeline is shown (#31): an identifier and the name to show.
pub type PersonFilter = RwSignal<Option<(String, String)>>;

#[component]
pub fn Timeline(refresh: RwSignal<u32>, person: PersonFilter) -> impl IntoView {
    let input = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let origin = RwSignal::new(None::<String>);
    let items = RwSignal::new(Vec::<StoredEvidence>::new());
    let has_more = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let origins = LocalResource::new(move || {
        refresh.track();
        api::evidence_origins()
    });

    let filter = move |offset: usize| EvidenceFilter {
        person: person.get_untracked().map(|(id, _)| id),
        q: Some(query.get_untracked()).filter(|q| !q.trim().is_empty()),
        origin: origin.get_untracked(),
        limit: Some(PAGE),
        offset: Some(offset),
    };
    let load = move |offset: usize| {
        loading.set(true);
        error.set(None);
        let f = filter(offset);
        spawn_local(async move {
            match api::list_evidence(f).await {
                Ok(page) => {
                    has_more.set(page.len() == PAGE);
                    if offset == 0 {
                        items.set(page);
                    } else {
                        items.update(|all| all.extend(page));
                    }
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        })
    };
    // A new search, another origin or new data: start again from the newest.
    Effect::new(move |_| {
        query.track();
        origin.track();
        person.track();
        refresh.track();
        load(0);
    });

    let search = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        query.set(input.get_untracked());
    };
    let chip = move |value: Option<String>, label: String| {
        let active = value.clone();
        view! {
            <button
                class="chip"
                class:active=move || origin.get() == active
                on:click=move |_| origin.set(value.clone())
            >
                {label}
            </button>
        }
    };

    view! {
        <section class="timeline" data-testid="timeline">
            {move || person.get().map(|(_, name)| view! {
                <div class="row">
                    <span class="chip active" data-testid="person-filter">{format!("Person: {name}")}</span>
                    <button class="icon-btn small" aria-label="alle zeigen" data-tip="Filter aufheben, alle zeigen" on:click=move |_| person.set(None)><Svg icon=Icon::Close /></button>
                </div>
            })}
            <form class="row" on:submit=search>
                <input
                    type="search"
                    aria-label="Rohdaten durchsuchen"
                    placeholder="Suchen in Nachrichten, Mails, Commits …"
                    prop:value=move || input.get()
                    on:input=move |ev| input.set(event_target_value(&ev))
                />
                <button type="submit" class="icon-btn primary" aria-label="Suchen" data-tip="Suchen (Enter)"><Svg icon=Icon::Search /></button>
            </form>
            {move || origins.get().and_then(|r| r.ok()).filter(|o| !o.is_empty()).map(|all| {
                let total: usize = all.iter().map(|o| o.count).sum();
                view! {
                    <div class="row origins">
                        {chip(None, format!("Alle ({total})"))}
                        {all.into_iter().map(|o| chip(Some(o.origin.clone()), format!("{} ({})", o.origin, o.count))).collect_view()}
                    </div>
                }
            })}
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            {move || {
                let all = items.get();
                if all.is_empty() {
                    let text = if loading.get() {
                        "Lade …"
                    } else if !query.get().trim().is_empty()
                        || origin.get().is_some()
                        || person.get().is_some()
                    {
                        "Nichts gefunden."
                    } else {
                        "Noch keine Rohdaten. Importieren mit symbiot-import, siehe docs/ROHDATEN.md."
                    };
                    return view! { <p class="muted">{text}</p> }.into_any();
                }
                let mut day = String::new();
                let rows = all.into_iter().map(|s| {
                    let this_day = date_of(s.evidence.at_ms);
                    let header = (this_day != day).then(|| {
                        day = this_day.clone();
                        view! { <h3 class="day">{this_day}</h3> }
                    });
                    view! { {header} <EvidenceRow item=s.evidence /> }
                }).collect_view();
                view! { <div class="evidence-list">{rows}</div> }.into_any()
            }}
            {move || has_more.get().then(|| view! {
                <button class="icon-btn more-btn" aria-label="Mehr laden" data-tip="Ältere laden" disabled=move || loading.get()
                    on:click=move |_| load(items.with_untracked(Vec::len))>
                    <Svg icon=Icon::More />
                </button>
            })}
        </section>
    }
}

#[component]
fn EvidenceRow(item: Evidence) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let origin = item.id.split_once(':').map_or("", |(o, _)| o).to_string();
    let who = sender(&item);
    let context = ["subject", "chat", "repo"]
        .iter()
        .find_map(|k| item.fields.iter().find(|(f, _)| f == k))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let long = item.text.chars().count() > PREVIEW;
    let text = item.text.clone();
    let shown = move || {
        if expanded.get() || !long {
            text.clone()
        } else {
            let cut: String = text.chars().take(PREVIEW).collect();
            format!("{cut} …")
        }
    };
    view! {
        <article class="evidence" data-testid="evidence" title=item.id.clone()>
            <div class="row">
                <span class="time">{time_of(item.at_ms)}</span>
                <span class="origin">{origin}</span>
                <strong>{who}</strong>
                <span class="muted">{context}</span>
            </div>
            {(!item.text.is_empty()).then(|| view! { <p class="text">{shown}</p> })}
            {long.then(|| view! {
                <button class="link" on:click=move |_| expanded.update(|e| *e = !*e)>
                    {move || if expanded.get() { "weniger" } else { "ganz lesen" }}
                </button>
            })}
            {(!item.files.is_empty()).then(|| view! {
                <div class="files">
                    {item.files.iter().cloned().map(|f| view! { <FileView file=f /> }).collect_view()}
                </div>
            })}
        </article>
    }
}

/// #28: a picture inline, any other file as a link (browser) or its name (app).
#[component]
fn FileView(file: FileRef) -> impl IntoView {
    let label = format!("{} ({})", file.name, size(file.len));
    let url = api::file_url(&file.hash);
    if api::is_picture(&file.mime) {
        let src = match url {
            Some(u) => Signal::derive(move || Some(u.clone())),
            None => {
                let hash = file.hash;
                let data = LocalResource::new(move || api::file_data_url(hash));
                Signal::derive(move || data.get().and_then(|r| r.ok()).flatten())
            }
        };
        let alt = file.name.clone();
        return view! {
            {move || match src.get() {
                Some(s) => view! { <img class="thumb" src=s alt=alt.clone() loading="lazy" /> }.into_any(),
                None => view! { <span class="muted file">{format!("🖼 {label}")}</span> }.into_any(),
            }}
        }
        .into_any();
    }
    match url {
        Some(u) => view! { <a class="file" href=u><Svg icon=Icon::Download />{label.clone()}</a> }
            .into_any(),
        None => view! { <span class="muted file">{label.clone()}</span> }.into_any(),
    }
}

fn size(len: u64) -> String {
    match len {
        n if n >= 1024 * 1024 => format!("{:.1} MB", n as f64 / (1024.0 * 1024.0)),
        n if n >= 1024 => format!("{} KB", n / 1024),
        n => format!("{n} B"),
    }
}

/// The sender or author as the source names them: the display name, else the id without scheme.
fn sender(e: &Evidence) -> String {
    let who = e
        .actors
        .iter()
        .find(|a| a.role == role::FROM || a.role == role::AUTHOR)
        .or(e.actors.first());
    match who {
        Some(a) if !a.name.is_empty() => a.name.clone(),
        Some(a) => {
            a.id.split_once(':')
                .map_or(a.id.as_str(), |(_, r)| r)
                .to_string()
        }
        None => String::new(),
    }
}

fn js_date(ms: u64) -> js_sys::Date {
    js_sys::Date::new(&JsValue::from_f64(ms as f64))
}

/// „Dienstag, 12. März 2024“ in the device's time zone.
fn date_of(ms: u64) -> String {
    let opts = js_sys::Object::new();
    for (k, v) in [
        ("weekday", "long"),
        ("day", "numeric"),
        ("month", "long"),
        ("year", "numeric"),
    ] {
        let _ = js_sys::Reflect::set(&opts, &k.into(), &v.into());
    }
    js_date(ms).to_locale_date_string("de-DE", &opts).into()
}

/// „14:05“ in the device's time zone.
fn time_of(ms: u64) -> String {
    let d = js_date(ms);
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}
