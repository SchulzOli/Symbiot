//! Symbiot UI — one Leptos app for desktop (Tauri), mobile and the web.

mod api;

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Node, NodeKind, VerifyReport};
use wasm_bindgen::JsValue;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    let filter = RwSignal::new(None::<NodeKind>);
    // Bumped after every write so the list reloads.
    let version = RwSignal::new(0u32);
    let nodes = LocalResource::new(move || {
        version.track();
        api::list_nodes(filter.get())
    });

    view! {
        <main>
            <header>
                <h1>"Symbiot"</h1>
                <p class="tagline">"Die KI schlägt vor, du entscheidest."</p>
            </header>
            <Capture on_saved=move || version.update(|v| *v += 1) />
            <Filter filter />
            <section class="nodes">
                {move || match nodes.get() {
                    None => view! { <p class="muted">"Lade …"</p> }.into_any(),
                    Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                    Some(Ok(list)) if list.is_empty() => {
                        view! { <p class="muted">"Noch nichts festgehalten."</p> }.into_any()
                    }
                    Some(Ok(list)) => list
                        .into_iter()
                        .map(|n| view! { <NodeCard node=n /> })
                        .collect_view()
                        .into_any(),
                }}
            </section>
            <Verify refresh=version />
            <footer>
                {format!(
                    "Daten: {} · core {} · protocol {}",
                    api::backend_label(),
                    symbiot_core::VERSION,
                    symbiot_core::PROTOCOL_VERSION,
                )}
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
fn NodeCard(node: Node) -> impl IntoView {
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
        <article class="node">
            <span class=format!("kind kind-{}", node.kind.as_str())>{node.kind.label()}</span>
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

fn short(hex: &str) -> String {
    hex.chars().take(8).collect()
}

fn format_ms(ms: u64) -> String {
    let d = js_sys::Date::new(&JsValue::from_f64(ms as f64));
    String::from(d.to_locale_string("de-DE", &JsValue::UNDEFINED))
}
