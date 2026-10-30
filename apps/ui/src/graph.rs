//! Graph view: nodes on a circle, grouped by kind, edges as arrows. Click selects a node.

use std::f64::consts::TAU;

use leptos::prelude::*;
use symbiot_core::{Edge, Node, NodeId};

const W: f64 = 640.0;
const H: f64 = 460.0;
const R: f64 = 170.0;

#[component]
pub fn Graph(
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    selected: RwSignal<Option<NodeId>>,
) -> impl IntoView {
    if nodes.is_empty() {
        return view! { <p class="muted">"Noch keine Knoten."</p> }.into_any();
    }

    let mut nodes = nodes;
    nodes.sort_by_key(|n| (n.kind, n.created_ms));
    let n = nodes.len() as f64;
    let pos = |i: usize| {
        if nodes.len() == 1 {
            return (W / 2.0, H / 2.0);
        }
        let a = TAU * i as f64 / n - TAU / 4.0;
        (W / 2.0 + R * a.cos(), H / 2.0 + R * a.sin())
    };
    let index_of = |id: &NodeId| nodes.iter().position(|x| x.id == *id);

    let lines = edges
        .iter()
        .filter_map(|e| {
            let (x1, y1) = pos(index_of(&e.from)?);
            let (x2, y2) = pos(index_of(&e.to)?);
            // Stop short of the target circle so the arrow head stays visible.
            let (dx, dy) = (x2 - x1, y2 - y1);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (x2, y2) = (x2 - dx / len * 22.0, y2 - dy / len * 22.0);
            let kind = e.kind;
            Some(view! {
                <line
                    class=format!("edge edge-{}", kind.as_str())
                    data-testid="graph-edge"
                    data-kind=kind.as_str()
                    x1=x1 y1=y1 x2=x2 y2=y2
                    marker-end=format!("url(#arrow-{})", kind.as_str())
                >
                    <title>{kind.label()}</title>
                </line>
            })
        })
        .collect_view();

    let circles = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let (x, y) = pos(i);
            let id = node.id;
            let label: String = if node.title.chars().count() > 22 {
                node.title.chars().take(21).chain(['…']).collect()
            } else {
                node.title.clone()
            };
            let below = y >= H / 2.0;
            view! {
                <g
                    class=format!("gnode kind-{}", node.kind.as_str())
                    class:selected=move || selected.get() == Some(id)
                    data-testid="graph-node"
                    on:click=move |_| selected.set(Some(id))
                >
                    <circle cx=x cy=y r="16">
                        <title>{format!("{}: {}", node.kind.label(), node.title)}</title>
                    </circle>
                    <text x=x y=if below { y + 34.0 } else { y - 24.0 } text-anchor="middle">
                        {label}
                    </text>
                </g>
            }
        })
        .collect_view();

    let markers = symbiot_core::EdgeKind::ALL
        .into_iter()
        .map(|k| {
            view! {
                <marker
                    id=format!("arrow-{}", k.as_str())
                    class=format!("edge-{}", k.as_str())
                    viewBox="0 0 10 10" refX="9" refY="5"
                    markerWidth="7" markerHeight="7" orient="auto-start-reverse"
                >
                    <path d="M0,0 L10,5 L0,10 z" />
                </marker>
            }
        })
        .collect_view();

    view! {
        <svg class="graph" viewBox=format!("0 0 {W} {H}") role="img" aria-label="Graph der Knoten und Verbindungen">
            <defs>{markers}</defs>
            {lines}
            {circles}
        </svg>
        <p class="legend">
            {symbiot_core::EdgeKind::ALL
                .into_iter()
                .map(|k| view! { <span class=format!("edge-{}", k.as_str())>{format!("— {}", k.label())}</span> })
                .collect_view()}
        </p>
    }
    .into_any()
}
