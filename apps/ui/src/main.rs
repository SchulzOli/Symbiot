//! Symbiot UI — one Leptos app for desktop (Tauri), mobile and the web.

mod api;
mod detail;
mod graph;

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Edge, Node, NodeId, NodeKind, VerifyReport};
use wasm_bindgen::JsValue;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    List,
    Graph,
}

async fn load() -> Result<(Vec<Node>, Vec<Edge>, Vec<NodeId>), String> {
    Ok((
        api::list_nodes(None).await?,
        api::list_edges().await?,
        api::list_sensitive().await?,
    ))
}

#[component]
fn App() -> impl IntoView {
    let filter = RwSignal::new(None::<NodeKind>);
    let mode = RwSignal::new(Mode::List);
    let selected = RwSignal::new(None::<NodeId>);
    // Bumped after every write so everything reloads.
    let version = RwSignal::new(0u32);
    let saved = move || version.update(|v| *v += 1);
    let data = LocalResource::new(move || {
        version.track();
        load()
    });

    view! {
        <main>
            <header>
                <h1>"Symbiot"</h1>
                <p class="tagline">"Die KI schlägt vor, du entscheidest."</p>
            </header>
            <Capture on_saved=saved />
            <nav class="row between">
                <Filter filter />
                <div class="tabs">
                    <button class="chip" class:active=move || mode.get() == Mode::List on:click=move |_| mode.set(Mode::List)>"Liste"</button>
                    <button class="chip" class:active=move || mode.get() == Mode::Graph on:click=move |_| mode.set(Mode::Graph)>"Graph"</button>
                </div>
            </nav>
            {move || match data.get() {
                None => view! { <p class="muted">"Lade …"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                Some(Ok((nodes, edges, sensitive))) => {
                    let detail = selected.get().and_then(|id| nodes.iter().find(|n| n.id == id).cloned()).map(|node| {
                        let is_sensitive = sensitive.contains(&node.id);
                        view! { <detail::Detail node nodes=nodes.clone() edges=edges.clone() sensitive=is_sensitive selected on_saved=saved refresh=version /> }
                    });
                    let shown: Vec<Node> = nodes
                        .iter()
                        .filter(|n| filter.get().is_none_or(|k| n.kind == k))
                        .cloned()
                        .collect();
                    let body = match mode.get() {
                        Mode::Graph => view! { <graph::Graph nodes=shown edges selected /> }.into_any(),
                        Mode::List if shown.is_empty() => view! { <p class="muted">"Noch nichts festgehalten."</p> }.into_any(),
                        Mode::List => view! {
                            <section class="nodes">
                                {shown.into_iter().map(|n| {
                                    let locked = sensitive.contains(&n.id);
                                    view! { <NodeCard node=n locked selected /> }
                                }).collect_view()}
                            </section>
                        }.into_any(),
                    };
                    view! { {detail} {body} }.into_any()
                }
            }}
            <Verify refresh=version />
            <footer>
                {format!(
                    "Daten: {} · core {} · protocol {} · ",
                    api::backend_label(),
                    symbiot_core::VERSION,
                    symbiot_core::PROTOCOL_VERSION,
                )}
                <a href=feedback_url() target="_blank" rel="noopener">"Feedback"</a>
            </footer>
        </main>
    }
}

/// F1 — capture a goal, tension, reflection, evidence, person or value.
#[component]
fn Capture(on_saved: impl Fn() + Copy + 'static) -> impl IntoView {
    let kind = RwSignal::new(NodeKind::Goal);
    let title = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        spawn_local(async move {
            match api::create_node(
                kind.get_untracked(),
                title.get_untracked(),
                body.get_untracked(),
            )
            .await
            {
                Ok(_) => {
                    title.set(String::new());
                    body.set(String::new());
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <form class="capture" on:submit=submit>
            <div class="row">
                <select on:change=move |ev| {
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
                    placeholder="Was beschäftigt dich?"
                    required
                    bind:value=title
                />
            </div>
            <textarea placeholder="Details (optional)" rows="2" bind:value=body></textarea>
            <div class="row end">
                {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
                <button type="submit" disabled=move || busy.get()>"Festhalten"</button>
            </div>
        </form>
    }
}

#[component]
fn Filter(filter: RwSignal<Option<NodeKind>>) -> impl IntoView {
    let chip = move |k: Option<NodeKind>, label: &'static str| {
        view! {
            <button
                class="chip"
                class:active=move || filter.get() == k
                on:click=move |_| filter.set(k)
            >
                {label}
            </button>
        }
    };
    view! {
        <nav class="filter">
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
            on:click=move |_| selected.set(Some(id))
        >
            <span class=format!("kind kind-{}", node.kind.as_str())>{node.kind.label()}</span>
            {locked.then(|| view! { <span class="lock" title="Sensibel – verlässt nie das Heimnetz">"🔒"</span> })}
            <h3>{node.title}</h3>
            {(!node.body.is_empty()).then(|| view! { <p>{node.body.clone()}</p> })}
            <small class="muted" title=node.id.to_hex()>{meta}</small>
        </article>
    }
}

/// F2 — verify this device's chain; shows the exact break point if anything was tampered with.
#[component]
fn Verify(refresh: RwSignal<u32>) -> impl IntoView {
    let report = RwSignal::new(None::<Result<VerifyReport, String>>);
    // A new write makes an old verdict stale.
    Effect::new(move |_| {
        refresh.track();
        report.set(None);
    });
    let check = move |_| spawn_local(async move { report.set(Some(api::verify().await)) });

    view! {
        <section class="verify">
            <button on:click=check>"Kette prüfen"</button>
            {move || report.get().map(|r| match r {
                Ok(r) if r.ok => view! {
                    <span class="ok" title=r.device.to_hex()>
                        {format!(
                            "✓ Kette ok · {} {} · Gerät {}",
                            r.entries,
                            if r.entries == 1 { "Eintrag" } else { "Einträge" },
                            short(&r.device.to_hex()),
                        )}
                    </span>
                }.into_any(),
                Ok(r) => view! {
                    <span class="error">{format!("✗ {}", r.error.unwrap_or_default())}</span>
                }.into_any(),
                Err(e) => view! { <span class="error">{e}</span> }.into_any(),
            })}
        </section>
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
