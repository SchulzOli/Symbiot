//! Detail of one node: edit it, see and add its connections, read its history.

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Edge, EdgeKind, Node, NodeId, NodeKind, Op, Proposal};

use crate::{api, format_ms, short};

#[component]
pub fn Detail(
    node: Node,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    sensitive: bool,
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
            <Sensitive id sensitive on_saved />
            {(node.kind == NodeKind::Goal).then(|| view! { <NextStep id nodes=nodes.clone() selected on_saved /> })}
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

/// E4: mark by hand, any time — a sensitive node never leaves the home network.
#[component]
fn Sensitive(
    id: NodeId,
    sensitive: bool,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let toggle = move |ev: leptos::ev::Event| {
        let on = event_target_checked(&ev);
        spawn_local(async move {
            match api::set_sensitive(id, on).await {
                Ok(()) => {
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        <label class="sensitive row">
            <input type="checkbox" prop:checked=sensitive on:change=toggle />
            "🔒 Sensibel – verlässt nie das Heimnetz"
            {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
        </label>
    }
}

/// F4: the AI suggests the next step, the human accepts or rejects it.
/// Until then the proposal lives only here, nothing is written.
#[component]
fn NextStep(
    id: NodeId,
    nodes: Vec<Node>,
    selected: RwSignal<Option<NodeId>>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let info = LocalResource::new(api::ai_info);
    let proposal = RwSignal::new(None::<Proposal>);
    let busy = RwSignal::new(false);
    let remote = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let ask = move |_| {
        busy.set(true);
        error.set(None);
        proposal.set(None);
        spawn_local(async move {
            match api::propose(id, remote.get_untracked()).await {
                Ok(p) => proposal.set(Some(p)),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let decide = move |accept: bool| {
        let Some(p) = proposal.get_untracked() else {
            return;
        };
        spawn_local(async move {
            let result = if accept {
                api::accept_proposal(p)
                    .await
                    .map(|a| selected.set(Some(a.node.id)))
            } else {
                api::reject_proposal(p).await
            };
            match result {
                Ok(()) => {
                    proposal.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let card = move || {
        proposal.get().map(|p| {
            let sources = p
                .sources
                .iter()
                .map(|s| {
                    let s = *s;
                    let label = nodes
                        .iter()
                        .find(|n| n.id == s)
                        .map(|n| format!("{}: {}", n.kind.label(), n.title))
                        .unwrap_or_else(|| short(&s.to_hex()));
                    view! { <li><button class="link" on:click=move |_| selected.set(Some(s))>{label}</button></li> }
                })
                .collect_view();
            view! {
                <div class="proposal" data-testid="proposal">
                    <h5>{p.title.clone()}</h5>
                    {(!p.body.is_empty()).then(|| view! { <p>{p.body.clone()}</p> })}
                    <p class="muted">"Quellen:"</p>
                    <ul class="sources">{sources}</ul>
                    <small class="muted">{format!("Modell {} · noch nicht gespeichert", p.model)}</small>
                    <div class="row end">
                        <button class="chip" on:click=move |_| decide(false)>"Ablehnen"</button>
                        <button on:click=move |_| decide(true)>"Annehmen"</button>
                    </div>
                </div>
            }
        })
    };

    view! {
        <h4>"Nächster Schritt"</h4>
        {move || match info.get() {
            None => view! { <p class="muted">"Lade …"</p> }.into_any(),
            Some(Ok(i)) if i.available => {
                let external = i.remote_model.clone().map(|m| view! {
                    <label class="row muted">
                        <input type="checkbox" prop:checked=move || remote.get() on:change=move |ev| remote.set(event_target_checked(&ev)) />
                        {format!("Externes Modell ({m}) – sensible Knoten bleiben zu Hause")}
                    </label>
                });
                view! {
                    <div class="next-step">
                        {external}
                        <button on:click=ask disabled=move || busy.get()>
                            {move || if busy.get() { "Denke nach …" } else { "Nächsten Schritt vorschlagen" }}
                        </button>
                    </div>
                }.into_any()
            }
            Some(_) => view! { <p class="muted">"KI ist nicht eingerichtet."</p> }.into_any(),
        }}
        {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        {card}
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
                        let title = match e.op() {
                            _ if e.is_pruned() => "Inhalt entfernt".to_string(),
                            Ok(Op::CreateNode { title, .. } | Op::UpdateNode { title, .. }) => {
                                format!("„{title}“")
                            }
                            Ok(Op::AcceptProposal { proposal }) => {
                                format!("„{}“ (KI-Vorschlag von {}, angenommen)", proposal.title, proposal.model)
                            }
                            Ok(_) => String::new(),
                            Err(err) => err.to_string(),
                        };
                        view! {
                            <li title=e.hash.to_hex()>
                                <strong>{format!("Version {}", i + 1)}</strong>
                                {format!(" · {} · {} · Eintrag {} · {}", format_ms(e.head.ts_ms), title, e.head.seq, short(&e.hash.to_hex()))}
                            </li>
                        }
                    })
                    .collect_view()
                    .into_any(),
            }}
        </ol>
    }
}
