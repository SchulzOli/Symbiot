//! #31: persons across all sources. Rules join spellings of the same identifier; everything
//! else is a suggestion, and only the human's click writes a decision into the chain.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, Person, Suggestion};
use crate::timeline::PersonFilter;

#[component]
pub fn Persons(
    refresh: RwSignal<u32>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
    show: PersonFilter,
) -> impl IntoView {
    let persons = LocalResource::new(move || {
        refresh.track();
        api::list_persons()
    });
    let suggestions = LocalResource::new(move || {
        refresh.track();
        api::person_suggestions()
    });
    let error = RwSignal::new(None::<String>);
    let run =
        move |job: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>>>>| {
            spawn_local(async move {
                match job.await {
                    Ok(()) => {
                        error.set(None);
                        on_saved();
                    }
                    Err(e) => error.set(Some(e)),
                }
            })
        };

    let merge = move |s: Suggestion| {
        let ids: Vec<String> =
            s.a.identifiers
                .iter()
                .chain(&s.b.identifiers)
                .map(|i| i.id.clone())
                .collect();
        // The longer name usually carries more („Anna Beispiel“ over „Anna“).
        let name = if s.b.name.chars().count() > s.a.name.chars().count() {
            s.b.name
        } else {
            s.a.name
        };
        run(Box::pin(api::merge_identities(ids, name)))
    };
    let apart = move |s: Suggestion| {
        run(Box::pin(api::keep_apart(
            s.a.identifiers[0].id.clone(),
            s.b.identifiers[0].id.clone(),
        )))
    };
    let split = move |id: String| run(Box::pin(api::split_identity(id)));

    view! {
        <section class="persons" data-testid="persons">
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            {move || suggestions.get().and_then(|r| r.ok()).filter(|s| !s.is_empty()).map(|all| view! {
                <div class="suggestions">
                    <h4>"Dieselbe Person?"</h4>
                    {all.into_iter().map(|s| {
                        let (m, a) = (s.clone(), s.clone());
                        view! {
                            <div class="card suggestion" data-testid="person-suggestion">
                                <p>
                                    <strong>{s.a.name.clone()}</strong>" "{who(&s.a)}
                                    " und "
                                    <strong>{s.b.name.clone()}</strong>" "{who(&s.b)}
                                    <span class="muted">{format!(" · {}", s.reason)}</span>
                                </p>
                                <div class="row">
                                    <button on:click=move |_| merge(m.clone())>"Zusammenführen"</button>
                                    <button class="chip" on:click=move |_| apart(a.clone())>"Nicht dieselbe"</button>
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            })}
            {move || match persons.get() {
                None => view! { <p class="muted">"Lade …"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                Some(Ok(all)) if all.is_empty() => view! {
                    <p class="muted">"Noch keine Personen. Sie kommen mit den Rohdaten (symbiot-import)."</p>
                }.into_any(),
                Some(Ok(all)) => view! {
                    <div class="person-list">
                        {all.into_iter().map(|p| view! { <PersonCard person=p show split /> }).collect_view()}
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

#[component]
fn PersonCard(
    person: Person,
    show: PersonFilter,
    split: impl Fn(String) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let several = person.identifiers.len() > 1;
    let (id, name) = (person.id.clone(), person.name.clone());
    view! {
        <article class="card person" data-testid="person">
            <div class="row between">
                <strong>{person.name.clone()}</strong>
                <span class="muted">{format!("{} · {}", count(person.count), person.origins.join(", "))}</span>
            </div>
            <div class="row identifiers">
                {person.identifiers.iter().map(|i| {
                    let id = i.id.clone();
                    view! {
                        <span class="identifier" title=i.names.join(", ")>
                            {i.id.clone()}
                            {several.then(|| view! {
                                <button class="link" title="Als eigene Person trennen" aria-label=format!("{} trennen", i.id)
                                    on:click=move |_| split(id.clone())>"×"</button>
                            })}
                        </span>
                    }
                }).collect_view()}
            </div>
            <button class="link" on:click=move |_| show.set(Some((id.clone(), name.clone())))>"Zeitleiste zeigen"</button>
        </article>
    }
}

/// „(WhatsApp, mail)“: where a person shows up, to tell two of the same name apart.
fn who(p: &Person) -> String {
    let ids: Vec<&str> = p
        .identifiers
        .iter()
        .map(|i| i.id.as_str())
        .take(2)
        .collect();
    format!("({})", ids.join(", "))
}

fn count(n: usize) -> String {
    if n == 1 {
        "1 Beleg".into()
    } else {
        format!("{n} Belege")
    }
}
