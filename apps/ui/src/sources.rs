//! #44: which raw data is used. Every source is shown as its pipeline — adapter → source →
//! used — with what it holds and the person's decision: pause, leave out chats or
//! repositories, only the last N days, or revoke (removes its raw data on every device).
//! Each change is a signed decision; the AI can suggest the same as actions (E16).

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::source::Grant;

use crate::api::{self, SourceInfo};
use crate::icons::{Icon, Svg};

/// Choices for „only the last N days“; 0 = all.
const DAYS: [(u32, &str); 5] = [
    (0, "alle"),
    (30, "30 Tage"),
    (90, "90 Tage"),
    (365, "1 Jahr"),
    (1825, "5 Jahre"),
];

#[component]
pub fn Sources(
    refresh: RwSignal<u32>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let sources = LocalResource::new(move || {
        refresh.track();
        api::list_sources()
    });
    let error = RwSignal::new(None::<String>);
    let status = RwSignal::new(String::new());

    view! {
        <section class="sources" data-testid="sources" aria-label="Quellen">
            <p class="muted">"Welche Rohdaten Symbiot nutzt. Jede Änderung ist eine signierte Entscheidung und gilt nach dem Abgleich auf allen Geräten."</p>
            <p class="sr-only" role="status" aria-live="polite">{move || status.get()}</p>
            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
            {move || match sources.get() {
                None => view! { <p class="muted">"Lade …"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                Some(Ok(all)) if all.is_empty() => view! {
                    <p class="muted">"Noch keine Quellen. Rohdaten kommen über Adapter, z. B. symbiot-import (siehe docs/ROHDATEN.md)."</p>
                }.into_any(),
                Some(Ok(all)) => view! {
                    <div class="pipelines">
                        {all.into_iter().map(|s| view! { <Pipeline info=s on_saved error status /> }).collect_view()}
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

#[component]
fn Pipeline(
    info: SourceInfo,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
    error: RwSignal<Option<String>>,
    status: RwSignal<String>,
) -> impl IntoView {
    let source = info.source.clone();
    let grant = info.grant.clone();
    let revoked = info.revoked_ms.is_some() && !grant.active && info.items == 0;
    let scopes_open = RwSignal::new(false);
    let confirm = RwSignal::new(false);
    let busy = RwSignal::new(false);

    let save = {
        let source = source.clone();
        move |g: Grant, done: String| {
            let source = source.clone();
            busy.set(true);
            spawn_local(async move {
                match api::grant_source(source, g).await {
                    Ok(()) => {
                        error.set(None);
                        status.set(done);
                        on_saved();
                    }
                    Err(e) => error.set(Some(e)),
                }
                busy.set(false);
            })
        }
    };
    let toggle_active = {
        let (save, grant, source) = (save.clone(), grant.clone(), source.clone());
        move |_| {
            let mut g = grant.clone();
            g.active = !g.active;
            let done = if g.active {
                format!("{source} wird genutzt")
            } else {
                format!("{source} pausiert")
            };
            save(g, done)
        }
    };
    let set_days = {
        let (save, grant, source) = (save.clone(), grant.clone(), source.clone());
        move |ev: leptos::ev::Event| {
            let Ok(days) = event_target_value(&ev).parse::<u32>() else {
                return;
            };
            let mut g = grant.clone();
            g.keep_days = days;
            save(g, format!("Zeitraum für {source} geändert"))
        }
    };
    let toggle_scope = {
        let (save, grant) = (save.clone(), grant.clone());
        move |name: String| {
            let mut g = grant.clone();
            let done = if let Some(i) = g.exclude.iter().position(|x| *x == name) {
                g.exclude.remove(i);
                format!("{name} wird genutzt")
            } else {
                g.exclude.push(name.clone());
                format!("{name} ausgelassen")
            };
            save(g, done)
        }
    };
    let revoke = {
        let source = source.clone();
        move |_| {
            let source = source.clone();
            busy.set(true);
            spawn_local(async move {
                match api::revoke_source(source.clone()).await {
                    Ok(n) => {
                        error.set(None);
                        status.set(format!("{source} widerrufen, {n} Rohdaten gelöscht"));
                        confirm.set(false);
                        on_saved();
                    }
                    Err(e) => error.set(Some(e)),
                }
                busy.set(false);
            })
        }
    };

    let state = if revoked {
        ("widerrufen", "revoked")
    } else if grant.active {
        ("aktiv", "active")
    } else {
        ("pausiert", "paused")
    };
    let heading_id = format!("source-{}", info.source);
    let labelled_by = heading_id.clone();
    let range = match (info.first_ms, info.last_ms) {
        (Some(a), Some(b)) => format!("{} – {}", day(a), day(b)),
        _ => String::new(),
    };
    let used_label = if info.used == info.items {
        format!("{} genutzt", count(info.used))
    } else {
        format!("{} von {} genutzt", info.used, dative(info.items))
    };
    let scopes = info.scopes.clone();
    let has_scopes = !scopes.is_empty();
    let excluded = grant.exclude.len();
    let keep_days = grant.keep_days;
    let active = grant.active;
    let name = info.source.clone();

    view! {
        <article class=format!("card pipeline state-{}", state.1) data-testid="source" aria-labelledby=labelled_by>
            <ol class="stages" aria-label="Weg der Daten">
                <li class="stage">
                    <span class="stage-label">"Adapter"</span>
                    {if info.adapters.is_empty() {
                        view! { <span class="muted">"–"</span> }.into_any()
                    } else {
                        info.adapters.iter().map(|a| view! {
                            <span class="adapter"><Svg icon=Icon::Devices />{a.clone()}</span>
                        }).collect_view().into_any()
                    }}
                </li>
                <li class="stage source-stage">
                    <span class="stage-label">"Quelle"</span>
                    <h3 id=heading_id>{info.source.clone()}</h3>
                    <span class=format!("badge {}", state.1)>{state.0}</span>
                    <small class="muted">{count(info.items)}{(!range.is_empty()).then(|| format!(" · {range}"))}</small>
                    {info.loaded_ms.map(|t| view! { <small class="muted">{format!("zuletzt geladen {}", crate::format_ms(t))}</small> })}
                </li>
                <li class="stage">
                    <span class="stage-label">"Genutzt"</span>
                    <strong data-testid="source-used">{used_label}</strong>
                    {(excluded > 0).then(|| view! { <small class="muted">{format!("{excluded} ausgelassen")}</small> })}
                    {(keep_days > 0).then(|| view! { <small class="muted">{format!("nur die letzten {keep_days} Tage")}</small> })}
                </li>
            </ol>
            <div class="row controls">
                <button class="icon-btn" class:primary=move || !active
                    aria-label=if active { format!("{name} pausieren") } else { format!("{name} nutzen") }
                    data-tip=if active { "Pausieren – Daten bleiben, werden aber nicht genutzt" } else { "Wieder nutzen" }
                    disabled=move || busy.get()
                    on:click=toggle_active>
                    <Svg icon=if active { Icon::Pause } else { Icon::Play } />
                </button>
                {has_scopes.then(|| view! {
                    <button class="icon-btn" aria-label="Auswahl" data-tip="Chats oder Repositories auslassen"
                        aria-expanded=move || scopes_open.get().to_string()
                        on:click=move |_| scopes_open.update(|o| *o = !*o)>
                        <Svg icon=Icon::Filter />
                    </button>
                })}
                <label class="days">
                    <Svg icon=Icon::Calendar />
                    <span class="sr-only">"Zeitraum"</span>
                    <select aria-label=format!("Zeitraum für {}", info.source) data-tip="Nur die letzten … nutzen"
                        disabled=move || busy.get() on:change=set_days>
                        {DAYS.into_iter().map(|(d, l)| view! {
                            <option value=d.to_string() selected=d == keep_days>{l}</option>
                        }).collect_view()}
                    </select>
                </label>
                <button class="icon-btn danger" aria-label=format!("{} widerrufen", info.source)
                    data-tip="Widerrufen – Rohdaten auf allen Geräten löschen"
                    disabled=move || busy.get() || info.items == 0
                    aria-expanded=move || confirm.get().to_string()
                    on:click=move |_| confirm.set(true)>
                    <Svg icon=Icon::Trash />
                </button>
            </div>
            {move || confirm.get().then(|| {
                let revoke = revoke.clone();
                view! {
                    <div class="confirm" role="alertdialog" aria-label="Widerrufen bestätigen">
                        <p>{format!("Alle Rohdaten von {} ({}) auf allen Geräten löschen? Das lässt sich nicht rückgängig machen.", info.source, count(info.items))}</p>
                        <div class="row">
                            <button class="icon-btn danger" aria-label="Endgültig löschen" data-tip="Endgültig löschen"
                                disabled=move || busy.get() on:click=revoke>
                                <Svg icon=Icon::Check />
                            </button>
                            <button class="icon-btn" aria-label="Abbrechen" data-tip="Abbrechen" on:click=move |_| confirm.set(false)>
                                <Svg icon=Icon::Close />
                            </button>
                        </div>
                    </div>
                }
            })}
            {move || scopes_open.get().then(|| {
                let toggle_scope = toggle_scope.clone();
                view! {
                    <fieldset class="scopes">
                        <legend>"Genutzt werden"</legend>
                        {scopes.iter().map(|sc| {
                            let toggle_scope = toggle_scope.clone();
                            let n = sc.name.clone();
                            view! {
                                <label class="scope">
                                    <input type="checkbox" prop:checked=!sc.excluded
                                        disabled=move || busy.get()
                                        on:change=move |_| toggle_scope(n.clone()) />
                                    {sc.name.clone()}
                                    <span class="muted">{format!(" ({})", sc.items)}</span>
                                </label>
                            }
                        }).collect_view()}
                    </fieldset>
                }
            })}
        </article>
    }
}

/// How the assistant names a grant (E16).
pub fn describe(source: &str, g: &Grant) -> String {
    let mut parts = vec![if g.active {
        format!("Quelle {source} nutzen")
    } else {
        format!("Quelle {source} pausieren")
    }];
    if !g.exclude.is_empty() {
        parts.push(format!("ohne {}", g.exclude.join(", ")));
    }
    if g.keep_days > 0 {
        parts.push(format!("nur die letzten {} Tage", g.keep_days));
    }
    parts.join(", ")
}

fn count(n: usize) -> String {
    if n == 1 {
        "1 Beleg".into()
    } else {
        format!("{n} Belege")
    }
}

/// „von 4 Belegen“.
fn dative(n: usize) -> String {
    if n == 1 {
        "1 Beleg".into()
    } else {
        format!("{n} Belegen")
    }
}

fn day(ms: u64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    String::from(d.to_locale_date_string("de-DE", &wasm_bindgen::JsValue::UNDEFINED))
}
