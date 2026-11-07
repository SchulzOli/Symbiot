//! M4: the person key (E5), the person's devices, and syncing with heimserver.

use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::PubKey;

use crate::api::{self, DeviceInfo, PersonInfo};
use crate::{format_ms, short};

#[component]
pub fn Devices(
    refresh: RwSignal<u32>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let info = LocalResource::new(move || {
        refresh.track();
        api::person()
    });
    view! {
        <section class="devices" data-testid="devices">
            <h4>"Geräte"</h4>
            {move || match info.get() {
                None => view! { <p class="muted">"Lade …"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                Some(Ok(p)) if p.person.is_none() => view! { <Setup on_saved /> }.into_any(),
                Some(Ok(p)) => view! {
                    <DeviceList info=p on_saved />
                    <SyncState on_saved />
                }
                .into_any(),
            }}
        </section>
    }
}

/// E5: a new person key (24 words, three of them checked) or an existing one.
#[component]
fn Setup(on_saved: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let fresh = RwSignal::new(String::new());
    let checks = RwSignal::new(Vec::<(usize, RwSignal<String>)>::new());
    let existing = RwSignal::new(String::new());
    let use_existing = RwSignal::new(false);
    let name = RwSignal::new(if api::in_app() { "PC" } else { "heimserver" }.to_string());
    let error = RwSignal::new(None::<String>);

    let start = move |_| {
        use_existing.set(false);
        error.set(None);
        spawn_local(async move {
            match api::new_words().await {
                Ok(w) => {
                    // Three different positions to type back, so the words were really noted.
                    let mut picks: Vec<usize> = Vec::new();
                    while picks.len() < 3 {
                        let i = (js_sys::Math::random() * 24.0) as usize % 24;
                        if !picks.contains(&i) {
                            picks.push(i);
                        }
                    }
                    picks.sort();
                    checks.set(
                        picks
                            .into_iter()
                            .map(|i| (i, RwSignal::new(String::new())))
                            .collect(),
                    );
                    fresh.set(w);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let certify = move |words: String| {
        spawn_local(async move {
            match api::certify(words, name.get_untracked()).await {
                Ok(_) => on_saved(),
                Err(e) => error.set(Some(e)),
            }
        })
    };
    let confirm_fresh = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let words = fresh.get_untracked();
        let list: Vec<&str> = words.split_whitespace().collect();
        for (i, typed) in checks.get_untracked() {
            if typed.get_untracked().trim().to_lowercase() != list[i] {
                error.set(Some(format!("Wort {} stimmt nicht", i + 1)));
                return;
            }
        }
        certify(words);
    };
    let confirm_existing = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        certify(existing.get_untracked());
    };

    let name_field = move || {
        view! {
            <label class="row">
                "Name dieses Geräts"
                <input type="text" required bind:value=name aria-label="Gerätename" />
            </label>
        }
    };

    view! {
        <div class="setup" data-testid="setup">
            <p>
                "Mit einem Personen-Schlüssel bestätigst du deine Geräte. Danach gleichen sie über heimserver ab, "
                "und ein verlorenes Gerät kannst du widerrufen."
            </p>
            <div class="row">
                <button on:click=start>"Personen-Schlüssel anlegen"</button>
                <button class="chip" on:click=move |_| { fresh.set(String::new()); use_existing.set(true); }>
                    "Vorhandenen Schlüssel eingeben"
                </button>
            </div>
            {move || (!fresh.get().is_empty()).then(|| view! {
                <form class="setup-fresh" on:submit=confirm_fresh>
                    <p class="hint">
                        "Schreib diese 24 Wörter auf oder leg sie im Passwort-Manager ab. Sie werden nur jetzt "
                        "gezeigt und nirgends gespeichert. Ohne sie kannst du kein weiteres Gerät einrichten."
                    </p>
                    <ol class="words" data-testid="words">
                        {fresh.get().split_whitespace().map(|w| view! { <li>{w.to_string()}</li> }).collect_view()}
                    </ol>
                    <div class="row">
                        {checks.get().into_iter().map(|(i, typed)| view! {
                            <label class="row">
                                {format!("Wort {}", i + 1)}
                                <input type="text" required bind:value=typed aria-label=format!("Wort {}", i + 1) />
                            </label>
                        }).collect_view()}
                    </div>
                    {name_field}
                    <div class="row end"><button type="submit">"Gerät bestätigen"</button></div>
                </form>
            })}
            {move || use_existing.get().then(|| view! {
                <form class="setup-existing" on:submit=confirm_existing>
                    <textarea rows="3" placeholder="Die 24 Wörter, durch Leerzeichen getrennt" bind:value=existing aria-label="Wiederherstellungs-Wörter"></textarea>
                    {name_field}
                    <div class="row end"><button type="submit">"Gerät bestätigen"</button></div>
                </form>
            })}
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        </div>
    }
}

#[component]
fn DeviceList(
    info: PersonInfo,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let rows = info
        .devices
        .into_iter()
        .map(|d| view! { <DeviceRow device=d on_saved /> })
        .collect_view();
    view! {
        <ul class="device-list">{rows}</ul>
        <small class="muted" title=info.person.map(|p| p.to_hex()).unwrap_or_default()>
            {format!("Personen-Schlüssel {}", info.person.map(|p| short(&p.to_hex())).unwrap_or_default())}
        </small>
    }
}

#[component]
fn DeviceRow(
    device: DeviceInfo,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let asking = RwSignal::new(false);
    let words = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let key: PubKey = device.key;
    let revoke = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        spawn_local(async move {
            match api::revoke(words.get_untracked(), key).await {
                Ok(_) => on_saved(),
                Err(e) => error.set(Some(e)),
            }
        })
    };
    let state = match (device.this_device, device.revoked_after) {
        (true, _) => " · dieses Gerät".to_string(),
        (_, Some(seq)) => format!(" · widerrufen nach Eintrag {seq}"),
        _ => String::new(),
    };
    let last = device
        .last_ms
        .map(|ms| format!(" · zuletzt {}", format_ms(ms)))
        .unwrap_or_default();
    let can_revoke = !device.this_device && device.revoked_after.is_none();
    view! {
        <li title=key.to_hex()>
            <strong>{device.name.clone()}</strong>
            {format!(" · {} · {} Einträge{last}{state}", short(&key.to_hex()), device.entries)}
            {can_revoke.then(|| view! {
                <button class="link danger-link" on:click=move |_| asking.set(true)>" Widerrufen"</button>
            })}
            {move || asking.get().then(|| view! {
                <form class="confirm" on:submit=revoke>
                    <p>
                        "Einträge, die dieses Gerät ab jetzt schreibt, nimmt kein Gerät mehr an. "
                        "Zum Bestätigen die 24 Wörter eingeben."
                    </p>
                    <textarea rows="2" required bind:value=words aria-label="Wiederherstellungs-Wörter"></textarea>
                    <div class="row end">
                        {move || error.get().map(|e| view! { <span class="error">{e}</span> })}
                        <button type="button" class="chip" on:click=move |_| asking.set(false)>"Abbrechen"</button>
                        <button type="submit" class="danger">"Gerät widerrufen"</button>
                    </div>
                </form>
            })}
        </li>
    }
}

/// In the app: the address of heimserver, when this device last synced, and a button to sync now.
/// In the browser the UI runs on heimserver itself, the hub.
#[component]
fn SyncState(on_saved: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    // `None` until loaded; `Some(None)` in the browser.
    let current = RwSignal::new(None::<Option<api::SyncStatus>>);
    let address = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    spawn_local(async move {
        match api::sync_status().await {
            Ok(s) => {
                address.set(s.as_ref().and_then(|s| s.url.clone()).unwrap_or_default());
                current.set(Some(s));
            }
            Err(e) => error.set(Some(e)),
        }
    });
    let finish = move |r: Result<api::SyncStatus, String>| {
        match r {
            Ok(s) => {
                if s.url.is_some() && s.last_error.is_none() {
                    on_saved();
                }
                error.set(None);
                current.set(Some(Some(s)));
            }
            Err(e) => error.set(Some(e)),
        }
        busy.set(false);
    };
    let sync = move |_| {
        busy.set(true);
        spawn_local(async move { finish(api::sync_now().await) })
    };
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        spawn_local(async move { finish(api::set_sync_url(address.get_untracked()).await) })
    };
    let describe = |s: &api::SyncStatus| -> String {
        match (s.last_ok_ms, s.last) {
            (Some(ms), Some(r)) => format!(
                "Zuletzt abgeglichen {} · {} gesendet, {} empfangen",
                format_ms(ms),
                r.sent,
                r.received
            ),
            _ => "Noch nicht abgeglichen".into(),
        }
    };
    view! {
        {move || match current.get() {
            None => view! { <span></span> }.into_any(),
            Some(None) => view! {
                <p class="muted">"Dies ist heimserver: Hier gleichen deine Geräte ab."</p>
            }
            .into_any(),
            Some(Some(s)) => view! {
                <form class="sync-address row" on:submit=save>
                    <label class="row">
                        "Adresse von heimserver"
                        <input
                            type="text"
                            placeholder="https://heimserver.example.ts.net:8449"
                            bind:value=address
                            aria-label="Adresse von heimserver"
                        />
                    </label>
                    <button type="submit" class="chip" disabled=move || busy.get()>"Speichern"</button>
                </form>
                {match s.url {
                    Some(_) => view! {
                        <div class="sync row">
                            <span class="muted">{describe(&s)}</span>
                            {s.last_error.clone().map(|e| view! { <span class="error">{e}</span> })}
                            <button class="chip" on:click=sync disabled=move || busy.get()>"Jetzt abgleichen"</button>
                        </div>
                    }
                    .into_any(),
                    None => view! {
                        <p class="muted">"Kein Abgleich eingerichtet: Adresse von heimserver eintragen und speichern."</p>
                    }
                    .into_any(),
                }}
            }
            .into_any(),
        }}
        {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
    }
}
