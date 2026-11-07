//! Detail of one node: edit it, see and add its connections, set its policy, read its
//! history, delete it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Edge, EdgeKind, Node, NodeId, NodeKind, Op, Policy, Proposal};

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
            let edge = e.id;
            let unlink = move |_| {
                spawn_local(async move {
                    if api::unlink(edge).await.is_ok() {
                        on_saved();
                    }
                })
            };
            view! {
                <li>
                    <button class="link" on:click=move |_| selected.set(Some(other))>{text}</button>
                    <button class="unlink" title="Verbindung lösen" aria-label="Verbindung lösen" on:click=unlink>"✕"</button>
                </li>
            }
        })
        .collect_view();

    let targets: Vec<(NodeId, String)> = nodes
        .iter()
        .filter(|n| n.id != id)
        .map(|n| (n.id, format!("{}: {}", n.kind.label(), n.title)))
        .collect();

    // E3: loaded once per detail, shared by the switches and the delete confirmation.
    let policy = RwSignal::new(Policy::default());
    spawn_local(async move {
        if let Ok(p) = api::node_policy(id).await {
            policy.set(p);
        }
    });

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
            {node.concurrent.then(|| view! { <Concurrent node=node.clone() on_saved /> })}
            <Edit node=node.clone() on_saved />
            <Sensitive id sensitive on_saved />
            <NodePolicy id policy on_saved />
            {(node.kind == NodeKind::Goal).then(|| view! { <NextStep id nodes=nodes.clone() selected on_saved /> })}
            <h4>"Verbindungen"</h4>
            <ul class="connections">{connections}</ul>
            <Connect id targets on_saved />
            <History id refresh />
            <Delete id policy selected on_saved />
        </aside>
    }
}

/// M4: another device changed the node at the same time. The newest version is shown; „Passt
/// so“ writes it again on top of itself, which clears the mark on every device.
#[component]
fn Concurrent(node: Node, on_saved: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let (id, title, body) = (node.id, node.title.clone(), node.body.clone());
    let keep = move |_| {
        let (title, body) = (title.clone(), body.clone());
        spawn_local(async move {
            match api::update_node(id, title, body).await {
                Ok(_) => on_saved(),
                Err(e) => error.set(Some(e)),
            }
        })
    };
    view! {
        <div class="concurrent-note" data-testid="concurrent">
            <p>
                "Gleichzeitig geändert: Ein anderes Gerät hat diesen Knoten bearbeitet, ohne die vorige "
                "Version zu kennen. Angezeigt wird die neueste, beide stehen im Verlauf."
            </p>
            <div class="row end">
                {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
                <button class="chip" on:click=keep>"Passt so"</button>
            </div>
        </div>
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
    // E4: why — by hand or by which rule.
    let why = LocalResource::new(move || api::node_sensitivity(id));
    let reason = move || {
        why.get()
            .and_then(|r| r.ok())
            .map(|w| match (w.by_hand, w.rules.is_empty()) {
                (Some(true), _) => "von Hand".to_string(),
                (Some(false), false) => format!(
                    "von Hand ausgenommen, obwohl Regel „{}“ passt",
                    w.rules.join("“, „")
                ),
                (Some(false), true) | (None, true) => String::new(),
                (None, false) => format!("durch Regel „{}“", w.rules.join("“, „")),
            })
    };
    view! {
        <label class="sensitive row">
            <input type="checkbox" prop:checked=sensitive on:change=toggle />
            "🔒 Sensibel – verlässt nie das Heimnetz"
            {move || reason().filter(|r| !r.is_empty()).map(|r| view! { <span class="muted">{format!("({r})")}</span> })}
            {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
        </label>
    }
}

/// E3: whether a node is deletable and exportable. Shared by capture and detail.
#[component]
pub fn PolicySwitches(
    policy: RwSignal<Policy>,
    on_change: impl Fn(Policy) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let set = move |apply: fn(&mut Policy, bool)| {
        move |ev: leptos::ev::Event| {
            let on = event_target_checked(&ev);
            policy.update(|p| apply(p, on));
            on_change(policy.get_untracked());
        }
    };
    view! {
        <div class="policy row">
            <label class="row">
                <input type="checkbox" prop:checked=move || policy.get().deletable on:change=set(|p, on| p.deletable = on) />
                "Löschbar"
            </label>
            <label class="row">
                <input type="checkbox" prop:checked=move || policy.get().exportable on:change=set(|p, on| p.exportable = on) />
                "Exportierbar"
            </label>
        </div>
        {move || (!policy.get().deletable).then(|| view! {
            <p class="hint">"Nicht löschbar: Löschen blendet den Knoten nur aus, sein Inhalt bleibt in der Kette."</p>
        })}
        {move || (!policy.get().exportable).then(|| view! {
            <p class="hint">"Nicht exportierbar: Der Knoten lebt nur auf diesem Gerät. Er geht in keinen Export, kein Backup und zu keiner externen KI. Geht das Gerät verloren, ist er weg."</p>
        })}
    }
}

/// E3: every change of the switches is a new signed entry, visible in the chain.
#[component]
fn NodePolicy(
    id: NodeId,
    policy: RwSignal<Policy>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let save = move |p: Policy| {
        spawn_local(async move {
            match api::set_policy(id, p).await {
                Ok(()) => {
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        })
    };
    view! {
        <PolicySwitches policy on_change=save />
        {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
    }
}

/// E2: deleting asks first and says honestly what happens to the content.
#[component]
fn Delete(
    id: NodeId,
    policy: RwSignal<Policy>,
    selected: RwSignal<Option<NodeId>>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let asking = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let delete = move |_| {
        spawn_local(async move {
            match api::delete_node(id).await {
                Ok(_) => {
                    selected.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        })
    };
    let consequence = move || {
        if policy.get().deletable {
            "Der Knoten und seine Verbindungen verschwinden. Titel und Text aller Versionen werden \
             auf diesem Gerät entfernt, die Kette bleibt prüfbar. Eigene Geräte räumen beim \
             Abgleich auf, fremde Geräte kann niemand dazu zwingen."
        } else {
            "Der Knoten ist nicht löschbar: Er wird nur ausgeblendet, sein Inhalt bleibt in der Kette."
        }
    };
    view! {
        <div class="row end">
            <button class="chip danger" on:click=move |_| asking.set(true)>"Löschen"</button>
        </div>
        {move || asking.get().then(|| view! {
            <div class="confirm" data-testid="confirm-delete">
                <p>{consequence}</p>
                <div class="row end">
                    {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
                    <button class="chip" on:click=move |_| asking.set(false)>"Abbrechen"</button>
                    <button class="danger" on:click=delete>"Endgültig löschen"</button>
                </div>
            </div>
        })}
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
    // M4: which device wrote each version.
    let people = LocalResource::new(api::person);
    let device_name = move |key: &symbiot_core::PubKey| {
        people
            .get()
            .and_then(|p| p.ok())
            .and_then(|p| p.devices.into_iter().find(|d| d.key == *key))
            .map(|d| d.name)
            .unwrap_or_else(|| short(&key.to_hex()))
    };
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
                                {format!(" · {} · {} · {} · Eintrag {} · {}", format_ms(e.head.ts_ms), title, device_name(&e.head.author), e.head.seq, short(&e.hash.to_hex()))}
                            </li>
                        }
                    })
                    .collect_view()
                    .into_any(),
            }}
        </ol>
    }
}
