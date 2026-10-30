//! Symbiot UI — one Leptos app for desktop (Tauri), mobile and the web.

use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    view! {
        <main>
            <h1>"Symbiot"</h1>
            <p>"Die KI schlägt vor, du entscheidest."</p>
            <footer>
                {format!("core {} · protocol {}", symbiot_core::VERSION, symbiot_core::PROTOCOL_VERSION)}
            </footer>
        </main>
    }
}
