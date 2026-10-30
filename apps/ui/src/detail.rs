//! Detail of one node: edit it, see and add its connections, read its history.

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Edge, EdgeKind, Node, NodeId, Op};

use crate::{api, format_ms, short};

#[component]
pub fn Detail(
    node: Node,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    selected: RwSignal<Option<NodeId>>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
    refresh: RwSignal<u32>,
) -> impl IntoView {
    let id = node.id;
    let title_of = {
        let nodes = nodes.clone();
        move |id: &NodeId| {
            nodes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| n.title.clone())
                .unwrap_or_else(|| short(&id.to_hex()))
        }
    };

    let connections = edges
        .iter()
        .filter(|e| e.from == id || e.to == id)
        .map(|e| {
            let text = if e.from == id {
                format!("→ {} „{}“", e.kind.label(), title_of(&e.to))
            } else {
                format!("← „{}“ {} dies", title_of(&e.from), e.kind.label())
            };
            let other = if e.from == id { e.to } else { e.from };
            view! {
                <li>
                    <button class="link" on:click=move |_| selected.set(Some(other))>{text}</button>
                </li>
            }
        })
        .collect_view();

    let targets: Vec<(NodeId, String)> = nodes
        .iter()
        .filter(|n| n.id != id)
        .map(|n| (n.id, format!("{}: {}", n.kind.label(), n.title)))
        .collect();

    // Bring the panel into view when a node far down the list is opened.
    let panel = NodeRef::<leptos::html::Aside>::new();
    Effect::new(move |_| {
        if let Some(el) = panel.get() {
            el.scroll_into_view();
        }
    });

    view! {
        <aside class="detail" data-testid="detail" node_ref=panel>
            <div class="row between">
                <span class=format!("kind kind-{}", node.kind.as_str())>{node.kind.label()}</span>
                <button class="chip" on:click=move |_| selected.set(None)>"Schließen"</button>
            </div>
            <Edit node=node.clone() on_saved />
            <h4>"Verbindungen"</h4>
            <ul class="connections">{connections}</ul>
            <Connect id targets on_saved />
            <History id refresh />
        </aside>
    }
}

/// Editing appends a new version; the old one stays in the ledger (see history below).
#[component]
fn Edit(node: Node, on_saved: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let title = RwSignal::new(node.title.clone());
    let body = RwSignal::new(node.body.clone());
    let error = RwSignal::new(None::<String>);
    let id = node.id;
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        spawn_local(async move {
            match api::update_node(id, title.get_untracked(), body.get_untracked()).await {
                Ok(_) => {
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        <form class="edit" on:submit=save>
            <input type="text" required bind:value=title aria-label="Titel" />
            <textarea rows="3" bind:value=body aria-label="Details"></textarea>
            <div class="row end">
                {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
                <button type="submit">"Neue Version speichern"</button>
            </div>
        </form>
    }
}

#[component]
fn Connect(
    id: NodeId,
    targets: Vec<(NodeId, String)>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    if targets.is_empty() {
        return view! { <p class="muted">"Lege einen zweiten Knoten an, um zu verbinden."</p> }
            .into_any();
    }
    let kind = RwSignal::new(EdgeKind::Supports);
    let target = RwSignal::new(Some(targets[0].0));
    let error = RwSignal::new(None::<String>);
    let connect = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(to) = target.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match api::link(id, to, kind.get_untracked()).await {
                Ok(_) => {
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        <form class="connect row" on:submit=connect>
            <span class="muted">"Dies"</span>
            <select aria-label="Art der Verbindung" on:change=move |ev| {
                if let Some(k) = EdgeKind::parse(&event_target_value(&ev)) {
                    kind.set(k);
                }
            }>
                {EdgeKind::ALL
                    .into_iter()
                    .map(|k| view! { <option value=k.as_str()>{k.label()}</option> })
                    .collect_view()}
            </select>
            <select aria-label="Ziel der Verbindung" on:change=move |ev| {
                target.set(NodeId::from_hex(&event_target_value(&ev)));
            }>
                {targets
                    .into_iter()
                    .map(|(id, label)| view! { <option value=id.to_hex()>{label}</option> })
                    .collect_view()}
            </select>
            <button type="submit">"Verbinden"</button>
            {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
        </form>
    }
    .into_any()
}

/// Every version of the node, straight from the ledger.
#[component]
fn History(id: NodeId, refresh: RwSignal<u32>) -> impl IntoView {
    let history = LocalResource::new(move || {
        refresh.track();
        api::node_history(id)
    });
    view! {
        <h4>"Verlauf"</h4>
        <ol class="history">
            {move || match history.get() {
                None => view! { <li class="muted">"Lade …"</li> }.into_any(),
                Some(Err(e)) => view! { <li class="error">{e}</li> }.into_any(),
                Some(Ok(entries)) => entries
                    .into_iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let title = match &e.op {
                            Op::CreateNode { title, .. } | Op::UpdateNode { title, .. } => {
                                title.clone()
                            }
                            Op::Link { .. } => String::new(),
                        };
                        view! {
                            <li title=e.hash.to_hex()>
                                <strong>{format!("Version {}", i + 1)}</strong>
                                {format!(" · {} · „{}“ · Eintrag {} · {}", format_ms(e.ts_ms), title, e.seq, short(&e.hash.to_hex()))}
                            </li>
                        }
                    })
                    .collect_view()
                    .into_any(),
            }}
        </ol>
    }
}
