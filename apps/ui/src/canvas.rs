//! The canvas (E15): the graph as cards on an endless surface — the start view.
//!
//! Built from DOM elements, not a `<canvas>` bitmap, so it stays accessible: every card is a
//! button in a list (screen readers read it in order, Tab walks it), every action has a
//! keyboard way (arrows pan, + / − zoom, 0 fits, Shift + arrows move the focused card,
//! L connects, Esc cancels). Positions are view state of this device and live in
//! `localStorage`; nothing about the layout goes into the chain.

use std::collections::HashMap;

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use symbiot_core::{Edge, EdgeKind, Node, NodeId, NodeKind};

use crate::api;
use crate::icons::{Icon, Svg};

const CARD_W: f64 = 220.0;
const CARD_H: f64 = 76.0;
const GAP_X: f64 = 60.0;
const GAP_Y: f64 = 28.0;
const STORE_KEY: &str = "symbiot.canvas.positions.v1";

type Positions = HashMap<NodeId, (f64, f64)>;

/// Pan and zoom. Kept by the app, so a reload after a change does not move the view.
#[derive(Clone, Copy, PartialEq)]
pub struct View {
    x: f64,
    y: f64,
    zoom: f64,
}

impl Default for View {
    fn default() -> Self {
        View {
            x: 24.0,
            y: 24.0,
            zoom: 1.0,
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn load_positions() -> Positions {
    let Some(raw) = storage().and_then(|s| s.get_item(STORE_KEY).ok().flatten()) else {
        return Positions::new();
    };
    raw.split(';')
        .filter_map(|e| {
            let mut p = e.split(',');
            let id = NodeId::from_hex(p.next()?)?;
            Some((id, (p.next()?.parse().ok()?, p.next()?.parse().ok()?)))
        })
        .collect()
}

fn save_positions(pos: &Positions) {
    let raw: Vec<String> = pos
        .iter()
        .map(|(id, (x, y))| format!("{},{x:.0},{y:.0}", id.to_hex()))
        .collect();
    if let Some(s) = storage() {
        let _ = s.set_item(STORE_KEY, &raw.join(";"));
    }
}

/// A place for every card: stored ones stay; new ones go on a ring, grouped by kind, so no
/// connection runs straight through another card. Large graphs get columns per kind.
fn layout(nodes: &[Node], stored: &Positions) -> Positions {
    let mut out = Positions::new();
    let fresh: Vec<&Node> = nodes
        .iter()
        .filter(|n| !stored.contains_key(&n.id))
        .collect();
    for n in nodes {
        if let Some(p) = stored.get(&n.id) {
            out.insert(n.id, *p);
        }
    }
    if fresh.len() <= 16 {
        let count = fresh.len().max(1) as f64;
        let radius = (count * (CARD_W + GAP_X) / std::f64::consts::TAU).max(170.0);
        for (i, n) in fresh.iter().enumerate() {
            let a = std::f64::consts::TAU * i as f64 / count - std::f64::consts::FRAC_PI_2;
            let (x, y) = if fresh.len() == 1 {
                (0.0, 0.0)
            } else {
                (radius * a.cos() * 1.3, radius * a.sin())
            };
            // Whole pixels: what is stored is exactly what is shown.
            out.insert(
                n.id,
                ((x - CARD_W / 2.0).round(), (y - CARD_H / 2.0).round()),
            );
        }
        return out;
    }
    let mut rows: HashMap<NodeKind, usize> = HashMap::new();
    for n in fresh {
        let col = NodeKind::ALL.iter().position(|k| *k == n.kind).unwrap_or(0) as f64;
        let row = rows.entry(n.kind).or_default();
        out.insert(
            n.id,
            (col * (CARD_W + GAP_X), *row as f64 * (CARD_H + GAP_Y)),
        );
        *row += 1;
    }
    out
}

/// Where the line from the centre `c` towards `other` leaves the card around `c`.
fn to_border(c: (f64, f64), other: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (other.0 - c.0, other.1 - c.1);
    if dx == 0.0 && dy == 0.0 {
        return c;
    }
    let tx = if dx == 0.0 {
        f64::MAX
    } else {
        (CARD_W / 2.0 + 6.0) / dx.abs()
    };
    let ty = if dy == 0.0 {
        f64::MAX
    } else {
        (CARD_H / 2.0 + 6.0) / dy.abs()
    };
    let t = tx.min(ty).min(1.0);
    (c.0 + dx * t, c.1 + dy * t)
}

#[component]
pub fn Canvas(
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    sensitive: Vec<NodeId>,
    selected: RwSignal<Option<NodeId>>,
    on_saved: impl Fn() + Copy + Send + Sync + 'static,
    /// The view and how many cards it was made for; `None` until the first fit.
    view: RwSignal<Option<(View, usize)>>,
) -> impl IntoView {
    let mut nodes = nodes;
    nodes.sort_by_key(|n| (n.kind, n.created_ms));
    let positions = RwSignal::new(layout(&nodes, &load_positions()));
    let count = nodes.len();
    let kept = view.get_untracked().filter(|(_, n)| *n == count);
    let view_state = RwSignal::new(kept.map(|(v, _)| v).unwrap_or_default());
    Effect::new(move |_| view.set(Some((view_state.get(), count))));
    let link_from = RwSignal::new(None::<NodeId>);
    let link_to = RwSignal::new(None::<NodeId>);
    let error = RwSignal::new(None::<String>);
    let surface = NodeRef::<leptos::html::Section>::new();

    // Dragging: a card (id, pointer start, card start) or the surface (pointer start, view start).
    let card_drag = StoredValue::new(None::<(NodeId, (f64, f64), (f64, f64), bool)>);
    let pan_drag = StoredValue::new(None::<((f64, f64), View)>);

    let zoom_by = move |factor: f64| {
        view_state.update(|v| v.zoom = (v.zoom * factor).clamp(0.3, 2.5));
    };
    let pan_by = move |dx: f64, dy: f64| {
        view_state.update(|v| {
            v.x += dx;
            v.y += dy;
        })
    };
    let fit = move || {
        let pos = positions.get_untracked();
        let Some(el) = surface.get_untracked() else {
            return;
        };
        if pos.is_empty() {
            view_state.set(View::default());
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (x, y) in pos.values() {
            x0 = x0.min(*x);
            y0 = y0.min(*y);
            x1 = x1.max(x + CARD_W);
            y1 = y1.max(y + CARD_H);
        }
        let rect = el.get_bounding_client_rect();
        let (w, h) = (rect.width().max(200.0), rect.height().max(200.0));
        let zoom = ((w - 48.0) / (x1 - x0))
            .min((h - 96.0) / (y1 - y0))
            .clamp(0.3, 1.2);
        view_state.set(View {
            x: (w - (x1 - x0) * zoom) / 2.0 - x0 * zoom,
            y: 72.0 - y0 * zoom,
            zoom,
        });
    };
    // The first view shows everything, and so does the view after a card came or went (so a
    // new card is never out of sight). Otherwise the view stays where the person left it.
    let first = kept.is_none();
    Effect::new(move |_| {
        if surface.get().is_some() && first {
            fit();
        }
    });

    let connect = move |from: NodeId, to: NodeId, kind: EdgeKind| {
        link_from.set(None);
        link_to.set(None);
        spawn_local(async move {
            match api::link(from, to, kind).await {
                Ok(_) => {
                    error.set(None);
                    on_saved();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let on_surface_key = move |ev: ev::KeyboardEvent| {
        let step = 48.0;
        match ev.key().as_str() {
            "ArrowLeft" if !ev.shift_key() => pan_by(step, 0.0),
            "ArrowRight" if !ev.shift_key() => pan_by(-step, 0.0),
            "ArrowUp" if !ev.shift_key() => pan_by(0.0, step),
            "ArrowDown" if !ev.shift_key() => pan_by(0.0, -step),
            "+" | "=" => zoom_by(1.2),
            "-" => zoom_by(1.0 / 1.2),
            "0" => fit(),
            "Escape" => {
                link_from.set(None);
                link_to.set(None);
                return;
            }
            _ => return,
        }
        ev.prevent_default();
    };
    let on_wheel = move |ev: ev::WheelEvent| {
        ev.prevent_default();
        if ev.ctrl_key() || ev.meta_key() {
            zoom_by(if ev.delta_y() < 0.0 { 1.1 } else { 1.0 / 1.1 });
        } else {
            pan_by(-ev.delta_x(), -ev.delta_y());
        }
    };
    let on_down = move |ev: ev::PointerEvent| {
        // Only the empty surface pans; cards handle their own pointer.
        let on_card = ev
            .target()
            .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok())
            .and_then(|e| {
                e.closest(".canvas-card, .canvas-tools, .link-kinds")
                    .ok()
                    .flatten()
            })
            .is_some();
        if on_card {
            return;
        }
        pan_drag.set_value(Some((
            (ev.client_x() as f64, ev.client_y() as f64),
            view_state.get_untracked(),
        )));
    };
    let on_move = move |ev: ev::PointerEvent| {
        let (cx, cy) = (ev.client_x() as f64, ev.client_y() as f64);
        if let Some((id, start, from, _)) = card_drag.get_value() {
            let zoom = view_state.get_untracked().zoom;
            let (dx, dy) = ((cx - start.0) / zoom, (cy - start.1) / zoom);
            if dx.abs() + dy.abs() > 4.0 {
                card_drag.set_value(Some((id, start, from, true)));
                positions.update(|p| {
                    p.insert(id, ((from.0 + dx).round(), (from.1 + dy).round()));
                });
            }
        } else if let Some((start, from)) = pan_drag.get_value() {
            view_state.set(View {
                x: from.x + cx - start.0,
                y: from.y + cy - start.1,
                ..from
            });
        }
    };
    let on_up = move |_: ev::PointerEvent| {
        if let Some((_, _, _, true)) = card_drag.get_value() {
            positions.with_untracked(save_positions);
        }
        pan_drag.set_value(None);
        // A drag must not count as a click: keep the flag until the click handler has seen it.
        if let Some((id, s, f, moved)) = card_drag.get_value() {
            card_drag.set_value(moved.then_some((id, s, f, true)));
        }
    };

    let count_edges = {
        let edges = edges.clone();
        move |id: NodeId| edges.iter().filter(|e| e.from == id || e.to == id).count()
    };

    let cards = nodes
        .iter()
        .map(|n| {
            let id = n.id;
            let locked = sensitive.contains(&id);
            let links = count_edges(id);
            let label = format!(
                "{}: {}{}, {} {}",
                n.kind.label(),
                n.title,
                if locked { ", sensibel" } else { "" },
                links,
                if links == 1 { "Verbindung" } else { "Verbindungen" }
            );
            let style = move || {
                let (x, y) = positions.with(|p| p.get(&id).copied().unwrap_or_default());
                format!("transform: translate({x}px, {y}px); width: {CARD_W}px;")
            };
            let click = move |_| {
                if matches!(card_drag.get_value(), Some((_, _, _, true))) {
                    card_drag.set_value(None);
                    return;
                }
                card_drag.set_value(None);
                match link_from.get_untracked() {
                    Some(from) if from != id => link_to.set(Some(id)),
                    Some(_) => link_from.set(None),
                    None => selected.set(Some(id)),
                }
            };
            let down = move |ev: ev::PointerEvent| {
                ev.stop_propagation();
                let from = positions.with_untracked(|p| p.get(&id).copied().unwrap_or_default());
                card_drag.set_value(Some((
                    id,
                    (ev.client_x() as f64, ev.client_y() as f64),
                    from,
                    false,
                )));
            };
            let key = move |ev: ev::KeyboardEvent| {
                let step = 24.0;
                let shift = ev.shift_key();
                let (dx, dy) = match ev.key().as_str() {
                    "ArrowLeft" if shift => (-step, 0.0),
                    "ArrowRight" if shift => (step, 0.0),
                    "ArrowUp" if shift => (0.0, -step),
                    "ArrowDown" if shift => (0.0, step),
                    "l" | "L" => {
                        ev.prevent_default();
                        ev.stop_propagation();
                        link_from.set(Some(id));
                        return;
                    }
                    _ => return,
                };
                ev.prevent_default();
                ev.stop_propagation();
                positions.update(|p| {
                    let e = p.entry(id).or_default();
                    e.0 += dx;
                    e.1 += dy;
                });
                positions.with_untracked(save_positions);
            };
            let start_link = move |ev: ev::MouseEvent| {
                ev.stop_propagation();
                link_from.set(Some(id));
            };
            view! {
                <li class="canvas-item" style=style>
                    <button
                        class=format!("canvas-card kind-border-{}", n.kind.as_str())
                        class:selected=move || selected.get() == Some(id)
                        class:linking=move || link_from.get() == Some(id)
                        data-testid="canvas-node"
                        aria-label=label
                        aria-keyshortcuts="Shift+ArrowLeft Shift+ArrowRight Shift+ArrowUp Shift+ArrowDown L"
                        on:click=click
                        on:pointerdown=down
                        on:keydown=key
                    >
                        <span class=format!("kind kind-{}", n.kind.as_str())>{n.kind.label()}</span>
                        <span class="card-title">{n.title.clone()}</span>
                        {locked.then(|| view! { <span class="lock-mark"><Svg icon=Icon::Lock /></span> })}
                    </button>
                    <button class="icon-btn card-link" aria-label=format!("„{}“ verbinden", n.title)
                        data-tip="Verbinden" on:click=start_link on:pointerdown=|ev| ev.stop_propagation()>
                        <Svg icon=Icon::Link />
                    </button>
                </li>
            }
        })
        .collect_view();

    let lines = {
        let edges = edges.clone();
        move || {
            positions.with(|p| {
                edges
                    .iter()
                    .filter_map(|e| {
                        let (x1, y1) = p.get(&e.from)?;
                        let (x2, y2) = p.get(&e.to)?;
                        let a = (x1 + CARD_W / 2.0, y1 + CARD_H / 2.0);
                        let b = (x2 + CARD_W / 2.0, y2 + CARD_H / 2.0);
                        // From border to border, so the arrow head is not under a card.
                        let (ax, ay) = to_border(a, b);
                        let (bx, by) = to_border(b, a);
                        Some(view! {
                            <line x1=ax y1=ay x2=bx y2=by
                                class=format!("edge edge-{}", e.kind.as_str())
                                data-testid="canvas-edge" data-kind=e.kind.as_str()
                                marker-end="url(#arrow)" />
                        })
                    })
                    .collect_view()
            })
        }
    };

    let world = move || {
        let v = view_state.get();
        format!(
            "transform: translate({}px, {}px) scale({});",
            v.x, v.y, v.zoom
        )
    };

    let titles: HashMap<NodeId, String> = nodes.iter().map(|n| (n.id, n.title.clone())).collect();
    let titles = StoredValue::new(titles);
    let title_of = move |id: NodeId| titles.with_value(|t| t.get(&id).cloned().unwrap_or_default());

    view! {
        <section
            class="canvas"
            class:linking=move || link_from.get().is_some()
            node_ref=surface
            tabindex="0"
            role="region"
            aria-label="Canvas mit deinen Knoten. Pfeiltasten verschieben die Ansicht, Plus und Minus zoomen, 0 zeigt alles."
            aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown + - 0 Escape"
            on:keydown=on_surface_key
            on:wheel=on_wheel
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointerleave=on_up
        >
            <div class="canvas-tools" role="toolbar" aria-label="Ansicht">
                <button class="icon-btn" aria-label="Vergrößern" data-tip="Vergrößern (+)" on:click=move |_| zoom_by(1.2)><Svg icon=Icon::ZoomIn /></button>
                <button class="icon-btn" aria-label="Verkleinern" data-tip="Verkleinern (−)" on:click=move |_| zoom_by(1.0 / 1.2)><Svg icon=Icon::ZoomOut /></button>
                <button class="icon-btn" aria-label="Alles zeigen" data-tip="Alles zeigen (0)" on:click=move |_| fit()><Svg icon=Icon::Fit /></button>
            </div>
            {move || link_from.get().map(|from| {
                let title = title_of(from);
                view! {
                    <p class="canvas-hint" role="status">
                        {format!("„{title}“ verbinden: Ziel anklicken oder mit Tab wählen und Enter drücken. Esc bricht ab.")}
                    </p>
                }
            })}
            {move || error.get().map(|e| view! { <p class="error canvas-hint" role="alert">{e}</p> })}
            {move || match (link_from.get(), link_to.get()) {
                (Some(from), Some(to)) => {
                    let (a, b) = (title_of(from), title_of(to));
                    Some(view! {
                        <div class="link-kinds card" role="dialog" aria-modal="false" aria-label="Art der Verbindung">
                            <p>{format!("„{a}“ …")}</p>
                            <div class="row">
                                {EdgeKind::ALL.into_iter().map(|k| view! {
                                    <button class=format!("chip edge-chip edge-chip-{}", k.as_str()) on:click=move |_| connect(from, to, k)>
                                        {format!("{} „{b}“", k.label())}
                                    </button>
                                }).collect_view()}
                                <button class="icon-btn" aria-label="Abbrechen" data-tip="Abbrechen (Esc)" on:click=move |_| { link_from.set(None); link_to.set(None); }>
                                    <Svg icon=Icon::Close />
                                </button>
                            </div>
                        </div>
                    })
                }
                _ => None,
            }}
            <div class="canvas-world" style=world>
                <svg class="canvas-edges" aria-hidden="true" width="1" height="1" overflow="visible">
                    <defs>
                        <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                            <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
                        </marker>
                    </defs>
                    {lines}
                </svg>
                <ul class="canvas-nodes" aria-label="Knoten">{cards}</ul>
            </div>
        </section>
    }
}
