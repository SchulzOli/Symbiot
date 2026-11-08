//! E16: the assistant. You say what should happen; the AI answers with a suggestion of
//! actions — the same actions you can do yourself. Nothing happens until you accept, and you
//! can leave out single actions. Dependent actions are kept consistent: leaving out a new
//! node leaves out what builds on it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::action::{Action, Suggestion, Target};
use symbiot_core::{Node, NodeId};

use crate::api;
use crate::icons::{Icon, Svg};

#[component]
pub fn Assistant(on_saved: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let info = LocalResource::new(api::ai_info);
    let nodes = LocalResource::new(|| api::list_nodes(None));
    let instruction = RwSignal::new(String::new());
    let remote = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let done = RwSignal::new(None::<String>);
    let suggestion = RwSignal::new(None::<Suggestion>);
    let chosen = RwSignal::new(Vec::<bool>::new());

    let ask = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        error.set(None);
        done.set(None);
        suggestion.set(None);
        spawn_local(async move {
            match api::assist(instruction.get_untracked(), remote.get_untracked()).await {
                Ok(s) => {
                    chosen.set(vec![true; s.actions.len()]);
                    suggestion.set(Some(s));
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    // Choosing an action chooses what it builds on; leaving one out leaves out what builds on it.
    let toggle = move |i: usize, on: bool| {
        let Some(s) = suggestion.get_untracked() else {
            return;
        };
        chosen.update(|c| {
            c[i] = on;
            let mut changed = true;
            while changed {
                changed = false;
                for (j, a) in s.actions.iter().enumerate() {
                    for t in a.targets() {
                        if let Target::New(n) = t {
                            let n = n as usize;
                            if c[j] && !c[n] && on {
                                c[n] = true;
                                changed = true;
                            }
                            if c[j] && !c[n] && !on {
                                c[j] = false;
                                changed = true;
                            }
                        }
                    }
                }
            }
        });
    };

    let decide = move |accept: bool| {
        let Some(s) = suggestion.get_untracked() else {
            return;
        };
        let picked: Vec<usize> = chosen.with_untracked(|c| {
            c.iter()
                .enumerate()
                .filter_map(|(i, on)| on.then_some(i))
                .collect()
        });
        busy.set(true);
        spawn_local(async move {
            match api::decide_suggestion(s, accept, picked).await {
                Ok(out) => {
                    suggestion.set(None);
                    instruction.set(String::new());
                    error.set(out.error.clone());
                    done.set(Some(if !accept {
                        "Abgelehnt. Nichts wurde geändert; die Entscheidung steht in der Kette."
                            .into()
                    } else {
                        let n = out.executed.len();
                        format!(
                            "{n} {} ausgeführt.",
                            if n == 1 { "Aktion" } else { "Aktionen" }
                        )
                    }));
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let available = move || info.get().and_then(|r| r.ok()).filter(|i| i.available);

    view! {
        <section class="assistant" data-testid="assistant">
            {move || match available() {
                None => view! { <p class="muted">"KI ist nicht eingerichtet."</p> }.into_any(),
                Some(i) => view! {
                    <form class="assist-form" on:submit=ask>
                        <label class="field">
                            <span>"Was soll geschehen?"</span>
                            <textarea rows="3" bind:value=instruction required
                                placeholder="z. B. Leg für „Laufen“ einen ersten Schritt an und verbinde ihn."></textarea>
                        </label>
                        <div class="row">
                            {i.remote_model.clone().map(|m| view! {
                                <label class="row muted">
                                    <input type="checkbox" prop:checked=move || remote.get()
                                        on:change=move |ev| remote.set(event_target_checked(&ev)) />
                                    {format!("Externes Modell ({m}) – Sensibles bleibt zu Hause")}
                                </label>
                            })}
                            <button type="submit" class="icon-btn primary" aria-label="Vorschlag holen"
                                data-tip="Vorschlag holen" disabled=move || busy.get()>
                                <Svg icon=Icon::Sparkles />
                            </button>
                            {move || busy.get().then(|| view! { <span class="muted" role="status">"Denke nach …"</span> })}
                        </div>
                    </form>
                }.into_any(),
            }}
            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
            {move || done.get().map(|d| view! { <p class="ok" role="status">{d}</p> })}
            {move || suggestion.get().map(|s| {
                let all = nodes.get().and_then(|r| r.ok()).unwrap_or_default();
                let lines: Vec<String> = s.actions.iter().map(|a| describe(a, &s, &all)).collect();
                view! {
                    <div class="card suggestion-card" data-testid="suggestion">
                        <h3>{s.title.clone()}</h3>
                        {(!s.reason.is_empty()).then(|| view! { <p>{s.reason.clone()}</p> })}
                        <fieldset>
                            <legend>"Aktionen"</legend>
                            {lines.into_iter().enumerate().map(|(i, line)| view! {
                                <label class="action-line">
                                    <input type="checkbox"
                                        prop:checked=move || chosen.with(|c| c.get(i).copied().unwrap_or(false))
                                        on:change=move |ev| toggle(i, event_target_checked(&ev)) />
                                    {line}
                                </label>
                            }).collect_view()}
                        </fieldset>
                        <small class="muted">{format!("Modell {} · noch nichts geändert", s.model)}</small>
                        <div class="row end">
                            <button class="icon-btn" aria-label="Ablehnen" data-tip="Ablehnen"
                                disabled=move || busy.get() on:click=move |_| decide(false)>
                                <Svg icon=Icon::Close />
                            </button>
                            <button class="icon-btn primary" aria-label="Ausgewählte ausführen" data-tip="Ausgewählte ausführen"
                                disabled=move || busy.get() || chosen.with(|c| !c.contains(&true))
                                on:click=move |_| decide(true)>
                                <Svg icon=Icon::Check />
                            </button>
                        </div>
                    </div>
                }
            })}
        </section>
    }
}

/// One action in words, with the titles of the nodes it is about.
fn describe(a: &Action, s: &Suggestion, nodes: &[Node]) -> String {
    let title = |t: &Target| -> String {
        match t {
            Target::Node(id) => title_of(id, nodes),
            Target::New(n) => match s.actions.get(*n as usize) {
                Some(Action::CreateNode { title, .. }) => format!("„{title}“ (neu)"),
                _ => format!("${n}"),
            },
        }
    };
    let yes = |b: bool| if b { "ja" } else { "nein" };
    match a {
        Action::CreateNode { kind, title, .. } => format!("Neu: {} „{title}“", kind.label()),
        Action::UpdateNode { node, title: t, .. } => format!("Ändern: {} → „{t}“", title(node)),
        Action::Link { from, to, kind } => {
            format!("Verbinden: {} {} {}", title(from), kind.label(), title(to))
        }
        Action::Unlink { .. } => "Eine Verbindung lösen".into(),
        Action::DeleteNode { node } => format!("Löschen: {}", title(node)),
        Action::SetSensitive {
            node,
            sensitive: true,
        } => format!("Sensibel: {}", title(node)),
        Action::SetSensitive { node, .. } => format!("Nicht mehr sensibel: {}", title(node)),
        Action::SetPolicy {
            node,
            deletable,
            exportable,
        } => format!(
            "Richtlinie für {}: löschbar {}, exportierbar {}",
            title(node),
            yes(*deletable),
            yes(*exportable)
        ),
        Action::AddRule { label, keywords } => {
            format!("Regel für Sensibles: {label} ({})", keywords.join(", "))
        }
        Action::RemoveRule { .. } => "Eine Regel für Sensibles entfernen".into(),
        Action::MergeIdentities { ids, .. } => format!("Eine Person: {}", ids.join(", ")),
        Action::SplitIdentity { id } => format!("Als eigene Person führen: {id}"),
        Action::KeepApart { a, b } => format!("Nicht dieselbe Person: {a} und {b}"),
    }
}

fn title_of(id: &NodeId, nodes: &[Node]) -> String {
    nodes
        .iter()
        .find(|n| n.id == *id)
        .map(|n| format!("„{}“", n.title))
        .unwrap_or_else(|| crate::short(&id.to_hex()))
}
