//! E4: rules for what is sensitive. The AI suggests, the human decides.

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Node, Rule};

use crate::api::{self, RuleProposal};

#[component]
pub fn Rules(
    refresh: RwSignal<u32>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let rules = LocalResource::new(move || {
        refresh.track();
        api::list_rules()
    });
    let info = LocalResource::new(api::ai_info);
    let proposals = RwSignal::new(Vec::<RuleProposal>::new());
    let nodes = RwSignal::new(Vec::<Node>::new());
    let busy = RwSignal::new(false);
    let asked = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let ask = move |_| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::suggest_rules().await {
                Ok(p) => {
                    nodes.set(api::list_nodes(None).await.unwrap_or_default());
                    proposals.set(p);
                    asked.set(true);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        })
    };
    let accept = move |p: RuleProposal| {
        spawn_local(async move {
            match api::add_rule(p.label.clone(), p.keywords.clone(), Some(p.model.clone())).await {
                Ok(_) => {
                    proposals.update(|all| all.retain(|x| x != &p));
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        })
    };
    // Declining only drops the suggestion here; nothing is written.
    let decline = move |p: RuleProposal| proposals.update(|all| all.retain(|x| x != &p));

    let card = move |p: RuleProposal| {
        let titles: Vec<String> = nodes.with(|all| {
            p.matches
                .iter()
                .filter_map(|id| all.iter().find(|n| n.id == *id).map(|n| n.title.clone()))
                .collect()
        });
        let (a, d) = (p.clone(), p.clone());
        view! {
            <div class="proposal" data-testid="rule-proposal">
                <h5>{p.label.clone()}</h5>
                <p class="muted">{format!("Stichwörter: {}", p.keywords.join(", "))}</p>
                <p class="muted">
                    {format!("Würde {} Knoten markieren: {}", p.matches.len(), titles.join(" · "))}
                </p>
                <small class="muted">{format!("Modell {} · noch nicht gespeichert", p.model)}</small>
                <div class="row end">
                    <button class="chip" on:click=move |_| decline(d.clone())>"Ablehnen"</button>
                    <button on:click=move |_| accept(a.clone())>"Annehmen"</button>
                </div>
            </div>
        }
    };

    view! {
        <section class="rules" data-testid="rules">
            <h4>"Sensibel"</h4>
            <p class="muted">
                "Regeln markieren Knoten als sensibel, die eines der Stichwörter enthalten. Sensible Knoten "
                "verlassen das Heimnetz nie. Eine Markierung von Hand gilt immer vor jeder Regel."
            </p>
            {move || match rules.get() {
                None => view! { <p class="muted">"Lade …"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                Some(Ok(list)) if list.is_empty() => view! { <p class="muted">"Noch keine Regel."</p> }.into_any(),
                Some(Ok(list)) => view! {
                    <ul class="rule-list">
                        {list.into_iter().map(|r| view! { <RuleRow rule=r on_saved /> }).collect_view()}
                    </ul>
                }
                .into_any(),
            }}
            {move || match info.get() {
                Some(Ok(i)) if i.available => view! {
                    <div class="row">
                        <button class="chip" on:click=ask disabled=move || busy.get()>
                            {move || if busy.get() { "Denke nach …" } else { "KI nach Regeln fragen" }}
                        </button>
                        <span class="muted">{format!("nur mit dem lokalen Modell ({})", i.local_model.unwrap_or_default())}</span>
                    </div>
                }
                .into_any(),
                _ => view! { <span></span> }.into_any(),
            }}
            {move || (asked.get() && proposals.get().is_empty()).then(|| view! {
                <p class="muted">"Keine neuen Vorschläge."</p>
            })}
            {move || proposals.get().into_iter().map(card).collect_view()}
            <OwnRule on_saved />
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        </section>
    }
}

#[component]
fn RuleRow(rule: Rule, on_saved: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let id = rule.id;
    let remove = move |_| {
        spawn_local(async move {
            match api::remove_rule(id).await {
                Ok(()) => on_saved(),
                Err(e) => error.set(Some(e)),
            }
        })
    };
    let origin = rule
        .model
        .as_ref()
        .map(|m| format!(" · vorgeschlagen von {m}"))
        .unwrap_or_default();
    view! {
        <li>
            <strong>{rule.label.clone()}</strong>
            {format!(": {}{origin}", rule.keywords.join(", "))}
            <button class="unlink" title="Regel entfernen" aria-label="Regel entfernen" on:click=remove>"✕"</button>
            {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
        </li>
    }
}

/// A rule of one's own: a name and keywords, separated by commas.
#[component]
fn OwnRule(on_saved: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let label = RwSignal::new(String::new());
    let keywords = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let words: Vec<String> = keywords
            .get_untracked()
            .split(',')
            .map(|k| k.trim().to_string())
            .collect();
        spawn_local(async move {
            match api::add_rule(label.get_untracked(), words, None).await {
                Ok(_) => {
                    label.set(String::new());
                    keywords.set(String::new());
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        })
    };
    view! {
        <form class="own-rule row" on:submit=save>
            <input type="text" placeholder="Eigene Regel, z. B. Finanzen" bind:value=label aria-label="Name der Regel" required />
            <input type="text" placeholder="Stichwörter, mit Komma: konto, steuer" bind:value=keywords aria-label="Stichwörter" required />
            <button type="submit" class="chip">"Regel hinzufügen"</button>
            {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
        </form>
    }
}
