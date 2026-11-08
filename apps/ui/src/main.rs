//! Symbiot UI — one Leptos app for desktop (Tauri), mobile and the web.
//!
//! E15: the canvas is the start view; actions are symbols with their words as label and
//! tooltip; everything works with keyboard and screen reader (landmarks, skip link, live
//! region for results, shortcuts listed in the tooltips).

mod api;
mod assistant;
mod canvas;
mod detail;
mod devices;
mod icons;
mod persons;
mod rules;
mod sources;
mod timeline;

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Edge, Node, NodeId, NodeKind, Policy, VerifyReport};
use wasm_bindgen::JsValue;

use icons::{Icon, Svg};

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// E15: the graph as cards on a surface — the start view.
    Canvas,
    /// The same nodes as a list: quick to scan, and the plainest way for assistive technology.
    List,
    /// #30: raw data (E14), not the graph.
    Timeline,
    /// #31: persons across all sources.
    Persons,
    /// #44: which raw data is used, per source.
    Sources,
}

impl Mode {
    const ALL: [Mode; 5] = [
        Mode::Canvas,
        Mode::List,
        Mode::Timeline,
        Mode::Persons,
        Mode::Sources,
    ];

    fn label(self) -> &'static str {
        match self {
            Mode::Canvas => "Canvas",
            Mode::List => "Liste",
            Mode::Timeline => "Zeitleiste",
            Mode::Persons => "Personen",
            Mode::Sources => "Quellen",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Mode::Canvas => Icon::Canvas,
            Mode::List => Icon::List,
            Mode::Timeline => Icon::Timeline,
            Mode::Persons => Icon::Persons,
            Mode::Sources => Icon::Sources,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Mode::Canvas => "1",
            Mode::List => "2",
            Mode::Timeline => "3",
            Mode::Persons => "4",
            Mode::Sources => "5",
        }
    }
}

/// Which side panel is open, besides the node detail.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    /// E16: say what should happen, get a suggestion of actions.
    Assistant,
    Rules,
    Devices,
}

async fn load() -> Result<(Vec<Node>, Vec<Edge>, Vec<NodeId>), String> {
    Ok((
        api::list_nodes(None).await?,
        api::list_edges().await?,
        api::list_sensitive().await?,
    ))
}

/// Keys typed into a field are text, not shortcuts.
fn typing(ev: &ev::KeyboardEvent) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|e| {
            matches!(e.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
                || e.get_attribute("contenteditable").is_some()
        })
}

#[component]
fn App() -> impl IntoView {
    let filter = RwSignal::new(None::<NodeKind>);
    let mode = RwSignal::new(Mode::Canvas);
    let panel = RwSignal::new(None::<Panel>);
    let canvas_view = RwSignal::new(None::<(canvas::View, usize)>);
    // #31: the person whose timeline is shown; choosing one switches to the timeline.
    let person = RwSignal::new(None::<(String, String)>);
    Effect::new(move |_| {
        if person.get().is_some() {
            mode.set(Mode::Timeline);
        }
    });
    let raw = move || matches!(mode.get(), Mode::Timeline | Mode::Persons | Mode::Sources);
    let selected = RwSignal::new(None::<NodeId>);
    // One side panel at a time: opening a node closes the others, and the other way round.
    Effect::new(move |_| {
        if selected.get().is_some() {
            panel.set(None);
        }
    });
    Effect::new(move |_| {
        if panel.get().is_some() {
            selected.set(None);
        }
    });
    // Bumped after every write so everything reloads.
    let version = RwSignal::new(0u32);
    let saved = move || version.update(|v| *v += 1);
    // M4: as long as this device is not set up, a note at the top leads to „Geräte“.
    let who = LocalResource::new(move || {
        version.track();
        api::person()
    });
    let set_up = move || who.get().and_then(|r| r.ok()).map(|p| p.person.is_some());
    let data = LocalResource::new(move || {
        version.track();
        load()
    });

    // Shortcuts: 1–5 views, N new entry, Esc closes the open panel.
    let title_input = NodeRef::<leptos::html::Input>::new();
    let _keys = window_event_listener(ev::keydown, move |ev| {
        if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
            return;
        }
        if ev.key() == "Escape" {
            if selected.get_untracked().is_some() {
                selected.set(None);
            } else {
                panel.set(None);
            }
            return;
        }
        if typing(&ev) {
            return;
        }
        if let Some(m) = Mode::ALL.into_iter().find(|m| m.key() == ev.key()) {
            mode.set(m);
            ev.prevent_default();
        } else if ev.key() == "a" || ev.key() == "A" {
            panel.set(Some(Panel::Assistant));
            ev.prevent_default();
        } else if (ev.key() == "n" || ev.key() == "N")
            && let Some(input) = title_input.get_untracked()
        {
            let _ = input.focus();
            ev.prevent_default();
        }
    });

    let toggle = move |p: Panel| panel.update(|open| *open = (*open != Some(p)).then_some(p));

    view! {
        <a class="skip-link" href="#main">"Zum Inhalt springen"</a>
        <div class="app">
            <header class="topbar">
                <div class="brand">
                    <h1>"Symbiot"</h1>
                    <span class="tagline">"Die KI schlägt vor, du entscheidest."</span>
                </div>
                <div class="toolbar" role="toolbar" aria-label="Werkzeuge">
                    <button class="icon-btn" aria-label="Assistent" data-tip="Assistent (A)"
                        aria-keyshortcuts="A"
                        aria-pressed=move || (panel.get() == Some(Panel::Assistant)).to_string()
                        on:click=move |_| toggle(Panel::Assistant)>
                        <Svg icon=Icon::Sparkles />
                    </button>
                    <Verify refresh=version />
                    <button class="icon-btn" aria-label="Regeln für Sensibles" data-tip="Regeln für Sensibles"
                        aria-pressed=move || (panel.get() == Some(Panel::Rules)).to_string()
                        on:click=move |_| toggle(Panel::Rules)>
                        <Svg icon=Icon::Rules />
                    </button>
                    <button class="icon-btn" aria-label="Geräte" data-tip="Geräte und Abgleich"
                        aria-pressed=move || (panel.get() == Some(Panel::Devices)).to_string()
                        on:click=move |_| toggle(Panel::Devices)>
                        <Svg icon=Icon::Devices />
                    </button>
                    <a class="icon-btn" href=feedback_url() target="_blank" rel="noopener"
                        aria-label="Feedback geben (öffnet GitHub)" data-tip="Feedback">
                        <Svg icon=Icon::Feedback />
                    </a>
                </div>
            </header>
            <nav class="rail" aria-label="Ansichten">
                {Mode::ALL.into_iter().map(|m| view! {
                    <button class="icon-btn" aria-label=m.label()
                        data-tip=format!("{} ({})", m.label(), m.key())
                        aria-keyshortcuts=m.key()
                        aria-pressed=move || (mode.get() == m).to_string()
                        on:click=move |_| mode.set(m)>
                        <Svg icon=m.icon() />
                    </button>
                }).collect_view()}
            </nav>
            <main id="main" tabindex="-1">
                {move || (set_up() == Some(false) && panel.get() != Some(Panel::Devices)).then(|| view! {
                    <div class="card banner" role="note" data-testid="setup-banner">
                        <Svg icon=Icon::Devices />
                        <span>"Dieses Gerät ist noch nicht eingerichtet. Mit einem Personen-Schlüssel gleichen deine Geräte ab."</span>
                        <button class="chip" on:click=move |_| panel.set(Some(Panel::Devices))>"Einrichten"</button>
                    </div>
                })}
                {move || (!raw()).then(|| view! {
                    <Capture on_saved=saved title_input />
                    <Filter filter />
                })}
                {move || (mode.get() == Mode::Timeline).then(|| view! { <timeline::Timeline refresh=version person /> })}
                {move || (mode.get() == Mode::Persons).then(|| view! { <persons::Persons refresh=version on_saved=saved show=person /> })}
                {move || (mode.get() == Mode::Sources).then(|| view! { <sources::Sources refresh=version on_saved=saved /> })}
                {move || match data.get() {
                    _ if raw() => ().into_any(),
                    None => view! { <p class="muted">"Lade …"</p> }.into_any(),
                    Some(Err(e)) => view! { <p class="error" role="alert">{e}</p> }.into_any(),
                    Some(Ok((nodes, edges, sensitive))) => {
                        let shown: Vec<Node> = nodes
                            .iter()
                            .filter(|n| filter.get().is_none_or(|k| n.kind == k))
                            .cloned()
                            .collect();
                        match mode.get() {
                            Mode::Canvas if nodes.is_empty() => view! {
                                <p class="empty muted">"Noch nichts festgehalten. Beginne oben mit einem Ziel, einer Spannung oder einem Gedanken."</p>
                            }.into_any(),
                            Mode::Canvas => view! {
                                <canvas::Canvas nodes=shown edges sensitive selected on_saved=saved view=canvas_view />
                            }.into_any(),
                            Mode::List if shown.is_empty() => view! { <p class="muted">"Noch nichts festgehalten."</p> }.into_any(),
                            Mode::List => view! {
                                <section class="nodes" aria-label="Knoten als Liste">
                                    {shown.into_iter().map(|n| {
                                        let locked = sensitive.contains(&n.id);
                                        view! { <NodeCard node=n locked selected /> }
                                    }).collect_view()}
                                </section>
                            }.into_any(),
                            Mode::Timeline | Mode::Persons | Mode::Sources => ().into_any(),
                        }
                    }
                }}
            </main>
            {move || data.get().and_then(|r| r.ok()).and_then(|(nodes, edges, sensitive)| {
                let node = selected.get().and_then(|id| nodes.iter().find(|n| n.id == id).cloned())?;
                let is_sensitive = sensitive.contains(&node.id);
                Some(view! { <detail::Detail node nodes=nodes.clone() edges sensitive=is_sensitive selected on_saved=saved refresh=version /> })
            })}
            {move || (panel.get() == Some(Panel::Assistant)).then(|| view! {
                <Sheet label="Assistent" close=move || panel.set(None)>
                    <assistant::Assistant on_saved=saved />
                </Sheet>
            })}
            {move || (panel.get() == Some(Panel::Rules)).then(|| view! {
                <Sheet label="Regeln für Sensibles" close=move || panel.set(None)>
                    <rules::Rules refresh=version on_saved=saved />
                </Sheet>
            })}
            {move || (panel.get() == Some(Panel::Devices)).then(|| view! {
                <Sheet label="Geräte und Abgleich" close=move || panel.set(None)>
                    <devices::Devices refresh=version on_saved=saved />
                </Sheet>
            })}
            <footer>
                {format!(
                    "Daten: {} · core {} · protocol {}",
                    api::backend_label(),
                    symbiot_core::VERSION,
                    symbiot_core::PROTOCOL_VERSION,
                )}
            </footer>
        </div>
    }
}

/// A side panel: on wide screens beside the content, on phones from the bottom.
#[component]
fn Sheet(
    label: &'static str,
    close: impl Fn() + Copy + Send + Sync + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <aside class="sheet" role="complementary" aria-label=label>
            <div class="sheet-head">
                <h2>{label}</h2>
                <button class="icon-btn" aria-label="Schließen" data-tip="Schließen (Esc)" on:click=move |_| close()>
                    <Svg icon=Icon::Close />
                </button>
            </div>
            {children()}
        </aside>
    }
}

/// F1 — capture a goal, tension, reflection, evidence, person or value.
#[component]
fn Capture(
    on_saved: impl Fn() + Copy + 'static,
    title_input: NodeRef<leptos::html::Input>,
) -> impl IntoView {
    let kind = RwSignal::new(NodeKind::Goal);
    let title = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let policy = RwSignal::new(Policy::default());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let more = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        spawn_local(async move {
            match api::create_node(
                kind.get_untracked(),
                title.get_untracked(),
                body.get_untracked(),
                policy.get_untracked(),
            )
            .await
            {
                Ok(_) => {
                    title.set(String::new());
                    body.set(String::new());
                    policy.set(Policy::default());
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <form class="capture" on:submit=submit aria-label="Festhalten">
            <div class="capture-row">
                <select aria-label="Art" on:change=move |ev| {
                    if let Some(k) = NodeKind::parse(&event_target_value(&ev)) {
                        kind.set(k);
                    }
                }>
                    {NodeKind::ALL
                        .into_iter()
                        .map(|k| view! { <option value=k.as_str() selected=move || kind.get() == k>{k.label()}</option> })
                        .collect_view()}
                </select>
                <input
                    type="text"
                    node_ref=title_input
                    placeholder="Was beschäftigt dich?"
                    aria-label="Titel"
                    aria-keyshortcuts="N"
                    required
                    bind:value=title
                />
                <button type="button" class="icon-btn" aria-label="Weitere Angaben"
                    data-tip="Details, löschbar, exportierbar"
                    aria-expanded=move || more.get().to_string()
                    class:open=move || more.get()
                    on:click=move |_| more.update(|m| *m = !*m)>
                    <Svg icon=Icon::More />
                </button>
                <button type="submit" class="icon-btn primary" aria-label="Festhalten" data-tip="Festhalten (Enter)"
                    disabled=move || busy.get()>
                    <Svg icon=Icon::Plus />
                </button>
            </div>
            {move || more.get().then(|| view! {
                <textarea placeholder="Details (optional)" aria-label="Details" rows="2" bind:value=body></textarea>
                <detail::PolicySwitches policy on_change=|_| () />
            })}
            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
        </form>
    }
}

#[component]
fn Filter(filter: RwSignal<Option<NodeKind>>) -> impl IntoView {
    let chip = move |k: Option<NodeKind>, label: &'static str| {
        view! {
            <button
                class=format!("chip{}", k.map(|k| format!(" chip-kind kind-dot-{}", k.as_str())).unwrap_or_default())
                aria-pressed=move || (filter.get() == k).to_string()
                on:click=move |_| filter.set(k)
            >
                {label}
            </button>
        }
    };
    view! {
        <nav class="filter" aria-label="Nach Art filtern">
            {chip(None, "Alle")}
            {NodeKind::ALL.into_iter().map(|k| chip(Some(k), k.label())).collect_view()}
        </nav>
    }
}

#[component]
fn NodeCard(node: Node, locked: bool, selected: RwSignal<Option<NodeId>>) -> impl IntoView {
    let id = node.id;
    let changed = format_ms(node.updated_ms);
    let meta = if node.version > 1 {
        format!(
            "{} · Version {} · {}",
            short(&node.id.to_hex()),
            node.version,
            changed
        )
    } else {
        format!("{} · {}", short(&node.id.to_hex()), changed)
    };
    view! {
        <article
            class="node"
            class:selected=move || selected.get() == Some(id)
            data-testid="node-card"
            tabindex="0"
            role="button"
            on:click=move |_| selected.set(Some(id))
            on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                if ev.key() == "Enter" || ev.key() == " " {
                    ev.prevent_default();
                    selected.set(Some(id));
                }
            }
        >
            <span class=format!("kind kind-{}", node.kind.as_str())>{node.kind.label()}</span>
            {locked.then(|| view! { <span class="lock" aria-label="Sensibel – verlässt nie das Heimnetz" data-tip="Sensibel – verlässt nie das Heimnetz"><Svg icon=Icon::Lock /></span> })}
            {node.concurrent.then(|| view! {
                <span class="concurrent" title="Ein anderes Gerät hat den Knoten gleichzeitig geändert. Beide Versionen stehen im Verlauf.">
                    "gleichzeitig geändert"
                </span>
            })}
            <h3>{node.title}</h3>
            {(!node.body.is_empty()).then(|| view! { <p>{node.body.clone()}</p> })}
            <small class="muted" title=node.id.to_hex()>{meta}</small>
        </article>
    }
}

/// F2 — verify every chain known here; the result is announced (live region) and shown as a
/// note under the top bar, with the exact break point if anything was tampered with.
#[component]
fn Verify(refresh: RwSignal<u32>) -> impl IntoView {
    let report = RwSignal::new(None::<Result<VerifyReport, String>>);
    let others = RwSignal::new(None::<Result<Vec<VerifyReport>, String>>);
    // A new write makes an old verdict stale.
    Effect::new(move |_| {
        refresh.track();
        report.set(None);
        others.set(None);
    });
    let check = move |_| {
        spawn_local(async move {
            report.set(Some(api::verify().await));
            others.set(Some(api::verify_all().await));
        })
    };
    let ok = move || report.get().map(|r| matches!(r, Ok(ref r) if r.ok));

    view! {
        <button class="icon-btn" aria-label="Kette prüfen" data-tip="Kette prüfen"
            class:ok=move || ok() == Some(true) class:bad=move || ok() == Some(false)
            on:click=check>
            <Svg icon=Icon::ShieldCheck />
        </button>
        <div class="verify" role="status" aria-live="polite" class:shown=move || report.get().is_some()>
            {move || report.get().map(|r| match r {
                Ok(r) if r.ok => view! {
                    <span class="ok" title=r.device.to_hex()>
                        {format!(
                            "✓ Kette ok · {} {}{} · Gerät {}",
                            r.entries,
                            if r.entries == 1 { "Eintrag" } else { "Einträge" },
                            if r.pruned > 0 {
                                format!(", davon {} ohne Inhalt (gelöscht)", r.pruned)
                            } else {
                                String::new()
                            },
                            short(&r.device.to_hex()),
                        )}
                    </span>
                }.into_any(),
                Ok(r) => view! {
                    <span class="error">{format!("✗ {}", r.error.unwrap_or_default())}</span>
                }.into_any(),
                Err(e) => view! { <span class="error">{e}</span> }.into_any(),
            })}
            // M4: the chains of the other devices known here.
            {move || others.get().map(|r| match r {
                Ok(all) if all.len() > 1 => match all.iter().find(|r| !r.ok) {
                    None => view! {
                        <span class="ok">{format!(" · alle {} Ketten ok", all.len())}</span>
                    }.into_any(),
                    Some(bad) => view! {
                        <span class="error">
                            {format!(" ✗ Kette von Gerät {}: {}", short(&bad.device.to_hex()), bad.error.clone().unwrap_or_default())}
                        </span>
                    }.into_any(),
                },
                Ok(_) => ().into_any(),
                Err(e) => view! { <span class="error">{e}</span> }.into_any(),
            })}
        </div>
    }
}

/// Feedback becomes a GitHub issue (M3 Rückkanal). Pre-filled, nothing is sent without the human.
fn feedback_url() -> String {
    let body = format!(
        "**Was ist passiert / was wünschst du dir?**\n\n\n---\nSymbiot core {} · protocol {} · {}",
        symbiot_core::VERSION,
        symbiot_core::PROTOCOL_VERSION,
        api::backend_label()
    );
    format!(
        "https://github.com/SchulzOli/Symbiot/issues/new?labels=feedback&title={}&body={}",
        String::from(js_sys::encode_uri_component("Feedback: ")),
        String::from(js_sys::encode_uri_component(&body)),
    )
}

pub(crate) fn short(hex: &str) -> String {
    hex.chars().take(8).collect()
}

pub(crate) fn format_ms(ms: u64) -> String {
    let d = js_sys::Date::new(&JsValue::from_f64(ms as f64));
    String::from(d.to_locale_string("de-DE", &JsValue::UNDEFINED))
}
