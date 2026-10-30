# Recherche: Adapter und Anzeige-Plugins für Symbiot

Stand: 2026-10-30 · Recherche, noch keine Entscheidung · ergänzt [TECH-STACK.md](TECH-STACK.md), [PLAN.md](PLAN.md)

**Frage:** Wie kommen Daten aus fremden Quellen in Symbiot? Gemeint sind Kalender, Mail und Messenger (Matrix),
Smart Home, Gesundheit und Wearables, Dateien, Web und offene Daten, Sensoren (ESP32) und andere Apps. Und wie
funktionieren „Anzeige-Plugins“, also eigene Ansichten für eine Art von Daten? Symbiot und „Connect“ sollen *ein*
System sein. Der Mensch entscheidet, wo jede Schicht läuft (eigenes Gerät, eigener Server, gehostet).

**Wie belastbar das ist:** Die meisten Angaben stammen aus Primärquellen (Spezifikationen, offizielle Doku,
RFCs). Einige Seiten (bytecodealliance.org, modelcontextprotocol.io) waren aus der Recherche-Umgebung nicht
direkt abrufbar. Dort stützen sich die Angaben auf Suchergebnisse, Spiegel oder GitHub. Sekundärquellen sind in
der Quellenliste mit *(sekundär)* markiert. Was ich nicht prüfen konnte, steht als *ungeprüft* im Text.

---

## (a) Kurzfassung

1. **Das Ketten-Modell von Symbiot ist schon das richtige Adapter-Modell.** Ein Adapter ist einfach *ein weiteres
   Gerät*. Er hat einen eigenen Ed25519-Schlüssel und eine eigene Kette mit nur einem Schreiber. Damit gibt es
   keine Schreibkonflikte, und jede Aussage hat eine prüfbare Herkunft. Kein anderes untersuchtes System bietet
   das fertig. Die Bausteine dafür (Signaturen, Delegation, Belege) kennt der Stand der Technik aber gut:
   Keyhive, SCITT, Verifiable Credentials.
2. **Laufzeit für fremden Code: WebAssembly-Komponenten auf wasmtime** (Component Model, WIT, WASI 0.2, später
   0.3). Das ist 2026 der reifste Weg zu *sandboxed* Plugins mit klar erklärten Fähigkeiten in Rust. Zed, Lapce
   und Spin nutzen ihn bereits. WASI 0.3 (natives async) ist seit Juni 2026 stabil, bei vielen Toolchains aber
   noch frisch.
3. **MCP ist die Schnittstelle zu KI-Agenten, nicht das Adapter-Format.** Symbiot sollte einen MCP-Server
   anbieten (lesen und *Vorschläge* anlegen, nie direkt schreiben). Fremde MCP-Server (z. B. Home Assistant)
   kann Symbiot als eine Adapter-Art einbinden. MCP wurde 2026 stark umgebaut (zustandslos, Tasks als
   Erweiterung), ist also noch beweglich.
4. **Anzeige-Plugins:** zuerst *deklarativ* (eine Ansichts-Beschreibung, die eingebaute Leptos-Bausteine
   darstellen). Danach *HTML im sandboxed iframe nach dem MCP-Apps-Standard* (`ui://`-Ressourcen,
   postMessage-JSON-RPC). So läuft dieselbe Ansicht in Symbiot, Claude und VS Code.
5. **Nichts davon ändert die Grundregel.** Adapter *beobachten* und schreiben Belege in ihre eigene Kette. Aus
   Belegen eigene Knoten zu machen und nach außen zu handeln (Mail senden, Licht schalten), bleibt ein
   Vorschlag, den der Mensch annimmt.
6. **Zuerst bauen:** (1) Kalender über ICS/CalDAV als erste WASM-Komponente, (2) Home Assistant als Sammelpunkt
   für Smart Home und Sensoren, (3) Matrix lesend. Dazu der eigene MCP-Server von Symbiot.

---

## (b) Vergleich der Optionen

Legende Reife: ●●● produktiv und stabil · ●●○ nutzbar, ändert sich noch · ●○○ früh oder Entwurf.
„Kette“ = Passung zu „jedes Gerät oder jeder Adapter signiert seine eigene Kette“.

### b1. Laufzeiten und Plugin-Modelle

| Option | Reife | Rust | no_std / Embedded | Sandbox | Passung „Kette“ |
|---|---|---|---|---|---|
| **wasmtime + Component Model (WIT, WASI 0.2/0.3)** | ●●○ (0.2 stabil seit 2024, 0.3 stabil seit 06/2026, in Wasmtime 46 Standard) | sehr gut (`wasmtime`, `wit-bindgen`, `wasip3`) | Wasmtime läuft `no_std` mit Pulley-Interpreter. Auf dem RP2350 belegt es ~300 KiB von 512 KiB, eher Experiment | stark: kein Zugriff ohne Import, Speicher getrennt, Fuel und Epochen begrenzen Rechenzeit | sehr gut: Der Host hält den Schlüssel und signiert, was die Komponente über `emit` meldet. Der Hash der Komponente steht im Eintrag |
| **Extism** (auf wasmtime) | ●●○ (Rust-SDK 1.x, 1.30 im Juni 2026) | gut | nein | gut: `allowed_hosts` und `allowed_paths` im Manifest, standardmäßig gesperrt | gut. Eigenes ABI statt WIT, also weniger Austausch mit dem Component-Ökosystem |
| **Native Rust im Prozess** (eingebaute Adapter) | ●●● | ja | ja (`symbiot-core`) | keine | gut, aber nur für eigenen, vertrauten Code |
| **Eigener Prozess / Container** (Airbyte-Stil: stdin/stdout, JSON-Zeilen) | ●●● | beliebig | nein | nur über OS oder Container | mittel: Der Prozess braucht den Schlüssel oder spricht über einen signierenden Host |
| **Tauri-2-Plugins** | ●●● | ja | nein | ACL für die *WebView* (Capabilities, Scopes). Plugin-Code selbst ist voll vertraut | nur App-Erweiterung, kein Adapter-Format für Dritte |
| **JS-Plugins ohne Sandbox** (Obsidian, VS Code) | ●●● | – | – | **keine.** Obsidian sagt selbst, dass es Plugins nicht beschränken kann | schlecht: widerspricht „nichts heimlich“ |
| **JS im iframe** (Logseq) | ●●○ | – | – | mittel. 2026 Ausbruch über das Host-DOM bei abgeschalteter CSP (CVE-2026-47901) | nur für Anzeige |
| **Envoy Dynamic Modules / proxy-wasm** (Vergleich) | ●○○ / ●●○ | Rust-SDK | – | Dynamic Modules gelten als „voll vertraut“. proxy-wasm ist sandboxed, aber langsamer | Lehre: Leistung gegen Isolation, Envoy macht beides |
| **Home-Assistant-Integration** (Python im HA-Prozess) | ●●● | – | – | keine, im Prozess. Qualitätsstufen und Config-Flow regeln die Bauweise | als *Quelle* ideal, als Modell zum Nachbauen nicht |

### b2. Protokolle für Integrationen

| Option | Reife | Rust | Embedded | Sicherheit / Rechte | Passung „Kette“ und Rolle für Symbiot |
|---|---|---|---|---|---|
| **MCP** (Model Context Protocol) | ●●○ (Spec 2025-11-25, dann 2026-07-28: zustandslos, Roots/Sampling/Logging veraltet, Tasks als Erweiterung) | offizielles `rmcp`-SDK und andere | nein | OAuth-basierte Autorisierung. Tool-Poisoning und Prompt-Injection sind bekannte Risiken | Schnittstelle zu Agenten: Symbiot als Server (Vorschläge), fremde Server als eine Adapter-Art |
| **MCP Apps** (SEP-1865, `ui://`) | ●●○ (stabil 2026-01-26, läuft in Claude, ChatGPT, VS Code, Goose) | über JS im iframe | – | sandboxed iframe, JSON-RPC über postMessage | Vorbild für Anzeige-Plugins |
| **MQTT** | ●●● | `rumqttc` u. a. | ja (ESP32) | TLS und Broker-ACL. Nachrichten sind nicht signiert | Transport für Sensoren. Die Signatur kommt aus `symbiot-core` im Payload |
| **Matter / Thread** | ●●○ | `rs-matter` (no_std, no-alloc, ESP32, nRF52840, RP2040) | ja | Gerätezertifikate (PKI). Steuerung lokal | später. Heute einfacher über Home Assistant |
| **Home Assistant** (REST, WebSocket, MCP-Server `mcp_server`) | ●●● | REST/WS mit `reqwest` | – | Token oder OAuth. Der MCP-Server kann nur einen Teil der MCP-Funktionen | **ein Adapter für hunderte Geräte** |
| **Matrix Appservice / Bridges** (mautrix `bridgev2`) | ●●● | `matrix-sdk` (Rust, E2EE) | nein | Appservice braucht Homeserver-Registrierung. Ein normaler Client-Login reicht zum Lesen | Matrix-Adapter liest Räume. Bridges holen Signal, WhatsApp usw. herein |
| **CalDAV / CardDAV** | ●●● | Bibliotheken vorhanden, aber dünn | – | HTTP-Auth | Kalender und Kontakte, überall verfügbar |
| **JMAP** (Mail RFC 8620/8621, Contacts RFC 9610, JSContact RFC 9553) | ●●○ (JMAP Calendars noch Entwurf `-26`) | wenige Crates | – | OAuth oder Basic | modernes Mail-Protokoll. JSCalendar und JSContact sind gute *Datenformate* |
| **ActivityPub / ActivityStreams 2.0** | ●●● (W3C-Rec. 2018) | `activitypub_federation` u. a. | – | HTTP-Signaturen | Vokabular für Ereignisse und Personen. Föderation erst in Phase WIR |
| **Solid / W3C Linked Web Storage** | ●○○ (LWS: erster öffentlicher Entwurf 03/2026, verspätet) | wenig | – | Solid-OIDC, ACL | Idee „Pod“ passt, Standard ist noch nicht reif. Beobachten |
| **Webhooks** | ●●● | axum | – | HMAC-Signatur der Absender | eingehend nur über Tailscale oder einen Relay-Adapter |
| **Airbyte-Protokoll** (spec/check/discover/read, RECORD/STATE) | ●●● | – (Docker, stdio) | – | Container | **bestes Vorbild für die Schnittstelle** eines Pull-Adapters |
| **Singer** | ●●○ (älter, Grundlage von Airbyte) | – | – | Prozess | wie Airbyte |
| **Nango** | ●●○ (ELv2, Self-Hosting teils nur Enterprise, Angaben widersprüchlich) | – (TypeScript) | – | speichert OAuth-Tokens zentral | höchstens als OAuth-Helfer. Widerspricht „lokal“ |
| **n8n / Node-RED** | ●●● | – | – | Workflows führen Aktionen *automatisch* aus. n8n unter „Sustainable Use License“ | nicht als Kern. Höchstens eine Quelle, die per Webhook liefert |
| **OpenAPI-generierte Connectoren** | ●●○ | `progenitor` | – | – | spart Code, aber ohne Rechte- oder Herkunftsmodell |

### b3. Herkunft, Einwilligung, Vokabular

| Option | Reife | Rust | Passung |
|---|---|---|---|
| **W3C Verifiable Credentials 2.0** (Rec. 15.05.2025, *Datum aus Sekundärquelle*) + Data Integrity `eddsa-jcs-2022` | ●●● | `ssi` (SpruceID) u. a. | Ed25519 ist schon da. Als **Exportformat** für Belege und Einwilligungen nach außen geeignet, intern zu schwer |
| **IETF SCITT** (RFC 9943, Architektur, 2026) | ●●○ | wenig, COSE/CBOR | Modell „Signed Statement → Transparenzdienst → Receipt“ passt genau zu sz als Kopie und Zeuge |
| **C2PA / Content Credentials** | ●●● (Medien) | `c2pa` (offizielles Rust-SDK) | nur für Bilder und Dokumente mit eingebetteter Herkunft. Prüfen beim Import, nicht als eigenes Format |
| **Data Transfer Project / DTI** | ●●○ | – (Java) | Exporte großer Dienste (Fotos, Playlists). Trust Registry seit 2026 nach Pilot. Eher Importquelle als Standard |
| **schema.org, ActivityStreams, JSCalendar/JSContact** | ●●● | Serde-Typen selbst schreiben | als **Typ-Vokabular** für Knoten: `schema:Event`, `as:Note`, `jscalendar:Event` |
| **Keyhive / Beelay** (Ink & Switch) | ●○○ (Forschung) | Rust | Capabilities und Delegation ohne Server, mit Ed25519 signiert. Genau das Problem „Adapter darf X“. Ideen übernehmen, Code noch nicht |
| **Automerge / Iroh** | ●●● / ●●● (Iroh 1.0 im Juni 2026) | Rust | Iroh: „Geräte über Schlüssel wählen, nicht über IPs“, passt zur Kette. Automerge erst nötig, wenn zwei Geräte denselben Knoten bearbeiten |
| **Anytype any-sync** | ●●○ | Go | lokal, Ende-zu-Ende verschlüsselt. Ein Plugin-API habe ich nicht gefunden |

---

## (c) Empfehlung für Symbiot

### c1. Leitidee: ein Adapter ist ein Gerät

```
 Quelle (CalDAV, HA, Matrix …)
        │  nur erlaubte Hosts
 ┌──────▼──────────────────────┐      Host (Desktop, sz oder gehostet)
 │ Adapter = WASM-Komponente   │ emit(record) ──► Host prüft gegen Manifest,
 │ exportiert: describe, sync, │                   signiert mit dem ADAPTER-Schlüssel,
 │   (act)                     │                   hängt an die Kette des Adapters an
 └─────────────────────────────┘                          │
                                                          ▼
   Kette Adapter „kalender@pc“: Beleg · Beleg · Beleg   (Beobachtungen, keine Entscheidungen)
                                                          │ Projektion
   KI / Regeln ──► Vorschlag „Termin X als Ziel-Schritt verknüpfen?“ ──► Mensch nimmt an
                                                          │
   Kette Gerät „pc“: Op::Link / Node (signiert vom Menschen-Gerät)
```

1. **Eigene Identität.** Jeder Adapter-*Einsatz* (Adapter × Konto × Ort, z. B. „kalender-privat@sz“) bekommt
   ein eigenes Ed25519-Schlüsselpaar und eine eigene Kette. Sync, Prüfung und Projektion funktionieren
   unverändert. Ein Adapter ist für den Kern ein Gerät wie jedes andere.
2. **Der Host signiert, nicht der Gast.** Der private Schlüssel bleibt im Host (Tauri-App oder Server). Die
   WASM-Komponente ruft nur `emit(record)` auf. So kann ein bösartiger Adapter den Schlüssel nicht stehlen. Er
   kann höchstens Unsinn in *seine* Kette schreiben, und die ist als „Adapter X sagt“ gekennzeichnet. Jeder
   Eintrag enthält den **SHA-256 des Komponenten-Binärs** und die Manifest-Version. Damit bleibt prüfbar,
   welcher Code eine Aussage erzeugt hat.
3. **Einwilligung als Eintrag.** Das Gerät des Menschen signiert in *seiner* Kette einen Eintrag
   `Op::Grant { adapter_key, component_hash, capabilities, scope, bis }`. Widerrufen heißt: ein neuer Eintrag
   `Op::Revoke`. Die Projektion akzeptiert Einträge einer Adapter-Kette nur, solange ein gültiger Grant besteht.
   Das ist ein kleines, eigenes Capability-Modell nach dem Vorbild von Keyhive und UCAN, ohne deren
   Abhängigkeiten. Es fügt sich in das ohnehin geplante `Consent` im Kern ein.
4. **Beobachten ist erlaubt, Entscheiden nicht.** Ein Adapter schreibt nur *Belege* (Evidence) in seine Kette.
   Erst der Mensch macht daraus eigene Knoten, Kanten oder Ziele, über einen Vorschlag der KI oder von Hand.
   Handlungen nach außen (Nachricht senden, Licht schalten) sind ebenfalls Vorschläge. Nach dem Klick führt der
   Adapter sie über ein eigenes Export `act(proposal_id)` aus, und nur mit der Capability `act:*`. Der Grant
   ist eine einmalige Erlaubnis zum *Beobachten*. Er erlaubt keine Automatik.
5. **Der Ort ist Konfiguration.** Dieselbe Komponente läuft auf dem Desktop, auf sz oder gehostet. Der Grant
   nennt den Ort. Der Mensch sieht pro Adapter: wo er läuft, welche Hosts er erreicht, wie viele Belege er
   geschrieben hat.

### c2. Adapter-Schnittstelle (WIT-Skizze)

Die Schnittstelle lehnt sich an das Airbyte-Protokoll an (`spec/check/discover/read` mit opakem `state`). Der
Unterschied: Datensätze sind typisierte Belege, keine Tabellenzeilen.

```wit
package symbiot:adapter@0.1.0;

interface types {
  record evidence {
    kind: string,            // Vokabular-IRI, z. B. "https://schema.org/Event"
    external-id: string,     // stabile ID in der Quelle (für Dubletten und Updates)
    observed-at: u64,        // Unix-ms
    payload: list<u8>,       // JSON (JSCalendar, JSContact, ActivityStreams …)
    source-uri: option<string>,
  }
}

world adapter {
  use types.{evidence};
  import emit: func(e: evidence) -> result<_, string>;        // Host prüft, signiert, hängt an
  import state-get: func() -> option<list<u8>>;               // Cursor, nur für diesen Adapter
  import state-set: func(s: list<u8>);
  import log: func(level: u8, msg: string);
  import wasi:http/outgoing-handler@0.2.x;                    // Host filtert nach allowed-hosts
  export describe: func() -> string;                          // Manifest: Name, Kinds, Capabilities, Config-Schema
  export check: func(config: string) -> result<_, string>;
  export sync: func(config: string) -> result<u32, string>;   // Anzahl gemeldeter Belege
}
```

- **Fähigkeiten im Manifest**, wie bei Zed (`process:exec`, `download_file` mit Scopes), Spin
  (`allowed_outbound_hosts`) und Extism (`allowed_hosts`): `net:https://caldav.example.org/*`,
  `kinds:[schema:Event]`, `act:none`. Standard ist „alles gesperrt“. Der Host setzt die Regeln durch,
  ausgehendes HTTP nur über einen eigenen Hook in `wasmtime-wasi-http`.
- **Geheimnisse sieht der Gast nicht.** OAuth-Tokens und Passwörter liegen im Host (OS-Keychain oder
  verschlüsselt in SQLite). Der Host setzt sie beim ausgehenden Request für die erlaubte Domain ein. Der
  Adapter kann ein Token also nicht an einen anderen Host schicken. *Muss im Prototyp geprüft werden.*
- **Grenzen:** wasmtime-Fuel oder Epochen-Unterbrechung, Speicherlimit, Zeitlimit pro `sync`. Keine Threads,
  kein Dateisystem außer optional einem vorgeöffneten Ordner (Datei-Adapter).
- **WASI-Version:** jetzt für WASI 0.2 bauen (alle Toolchains). Auf 0.3 umsteigen, sobald `wit-bindgen` und
  `wasip3` für Rust-Gäste stabil sind. `wasmtime` kann 0.2- und 0.3-Komponenten im selben Binary ausführen.
- **Mobil:** iOS verbietet JIT. Dort wäre der Pulley-Interpreter nötig (langsamer). Unter Android geht
  Cranelift. *Binärgröße und Laufzeit auf dem Handy ungeprüft.* Pragmatisch laufen Adapter zuerst auf
  Desktop und sz. Das Handy synchronisiert nur Ketten.

### c3. Drei Stufen von Adaptern (ehrlich nach Vertrauen)

| Stufe | Was | Sandbox | Beispiele |
|---|---|---|---|
| **0 – eingebaut** | Rust-Code im Repo, native | keine (eigener Code, Review) | Datei- und Ordner-Import, Matrix (E2EE mit `matrix-sdk` ist im WASM zu schwer), Webhook-Empfang |
| **1 – Komponente** | WASM-Komponente gegen `symbiot:adapter` | wasmtime, Capabilities | Kalender (ICS/CalDAV), Home Assistant REST, offene Daten (Wetter, ÖPNV), RSS |
| **2 – extern** | Fremder Dienst, mit dem ein Stufe-0- oder Stufe-1-Adapter spricht | Prozess oder Container, außerhalb von Symbiot | MCP-Server (HA `mcp_server`), Matrix-Bridges, n8n per Webhook |

Wichtig für Stufe 2: Die Kette gehört immer dem *Symbiot-Adapter*, der mit dem fremden Dienst spricht. Er
signiert „HA meldete um 14:03: Temperatur 21,4 °C“. Der fremde Dienst bekommt nie einen Symbiot-Schlüssel.

**Sensoren (ESP32)** sind keine Adapter. Sie sind echte Geräte mit `symbiot-core` und eigener Kette (F7). MQTT
ist dafür nur der Transport. Die Signatur steckt im Payload, sz prüft sie vor dem Speichern. Bei hoher Frequenz
fasst man Messwerte zusammen: ein Eintrag pro Minute mit Werten oder einem Merkle-Root, sonst wächst die Kette
zu schnell.

### c4. Daten-Mapping: Beleg → typisierter Knoten

- **Beleg-Knoten** (`Beleg`) bekommen ein `kind` als IRI aus etablierten Vokabularen: schema.org (Event, Person,
  Place, Observation), ActivityStreams (Note, Message), JSCalendar und JSContact (Kalender, Kontakte). Keine
  eigene Ontologie erfinden. Eine kleine Tabelle `kind → Anzeige → erlaubte Kanten` im Kern genügt.
- **Herkunft pro Beleg:** Adapter-Schlüssel, Komponenten-Hash, `external-id`, `observed-at`, `source-uri`,
  SHA-256 des Rohpayloads. Den Rohpayload optional als Blob aufbewahren. So beantwortet Symbiot „woher weißt
  du das?“ bis zur Quelle (Regel F4: Vorschläge zitieren Quellen).
- **Updates und Löschungen der Quelle** werden zu neuen Belegen mit derselben `external-id`, nie zu
  Änderungen. Die Projektion zeigt den neuesten Stand und den Verlauf.
- **Export nach außen:** Ein Beleg oder Grant lässt sich später als **VC 2.0 mit `eddsa-jcs-2022`** ausgeben,
  denn derselbe Ed25519-Schlüssel genügt. Für „sz bezeugt, dass X zum Zeitpunkt T vorlag“ ist **SCITT
  (RFC 9943)** das passende Modell: sz stellt eine Quittung (Receipt) aus. Beides ist ein späterer Schritt und
  ändert das interne Format nicht.

### c5. MCP: Symbiot spricht mit Agenten

- **Symbiot als MCP-Server** (`symbiot mcp`, wie die anderen Rust-Werkzeuge des Besitzers): Tools wie
  `search_nodes`, `get_node_with_provenance`, `propose_node`, `propose_link`. **Es gibt keine Schreib-Tools.**
  Jeder Aufruf von außen erzeugt höchstens einen Vorschlag mit Quellen. Damit passt die Grundregel auch auf
  fremde Agenten, und die „tödliche Dreierkombination“ (fremde Inhalte + private Daten + Weg nach außen) ist
  an der Stelle unterbrochen, an der Daten verändert würden.
- **Symbiot als MCP-Client:** Eine eingebaute Adapter-Art „MCP-Quelle“ ruft *lesende* Tools eines fremden
  MCP-Servers auf und schreibt die Antworten als Belege. Home Assistant hat einen offiziellen `mcp_server`
  (Streamable HTTP, OAuth). Tool-Beschreibungen gelten als nicht vertrauenswürdig: Version und Hash beim Grant
  festhalten und bei Änderung neu fragen (Schutz gegen „Rug Pull“).
- MCP ändert sich schnell: Mit 2026-07-28 sind Handshake und Sitzungen weg, Tasks wurden zur Erweiterung. Darum
  MCP nur an den Rändern einsetzen und das interne Adapter-Format davon getrennt halten.

### c6. Anzeige-Plugins

| Stufe | Wie | Wofür |
|---|---|---|
| **A – deklarativ** (zuerst) | Ein `view`-Manifest pro `kind`: welche Felder, welcher eingebaute Baustein (Liste, Zeitleiste, Kalender, Kurve, Karte, Karte-mit-Zitaten). Leptos zeichnet es, kein fremder Code | 80 % der Fälle: Termine, Messreihen, Nachrichten |
| **B – MCP-Apps-kompatibel** | HTML-Ressource `ui://…` im **sandboxed iframe** (`sandbox="allow-scripts"`, eigene strenge CSP, kein Netz). Daten kommen nur über postMessage-JSON-RPC vom Host. „Aktionen“ werden zu Vorschlägen | Sonderansichten wie Schlafphasen-Grafik oder Grundriss mit Sensoren. Dieselbe Ansicht läuft in Claude, VS Code usw. |
| **C – Rust/WASM-Rendering** | Komponente liefert einen Ansichtsbaum, Leptos zeichnet ihn | *nicht empfohlen*: kein Standard, viel eigener Aufwand |

Ein Anzeige-Plugin ist nur lesend und hat keine eigene Kette. Es wird aber mit Hash installiert und per Grant
freigegeben, damit „was zeigt mir was?“ prüfbar bleibt. Die Logseq-Lücke von 2026 zeigt, worauf es ankommt:
iframe *mit* CSP, keine Attribute in den Host-DOM übernehmen.

### c7. Was zuerst bauen (nach M3, vor oder parallel zu M4)

1. **Kalender (ICS-Abo, dann CalDAV) als erste WASM-Komponente.** Klar definierte Daten (JSCalendar), nur
   lesend, ein erlaubter Host. Das zeigt den ganzen Weg: Manifest, Grant, Sandbox, `emit`, Signatur durch den
   Host, Cursor, Beleg-Knoten, deklarative Kalender-Ansicht, KI-Vorschlag „Termin stützt Ziel X“.
   *Fertig, wenn:* Ein Test zeigt, dass der Adapter keinen anderen Host erreicht und ohne Grant nichts in der
   Projektion erscheint.
2. **Home Assistant (REST/WebSocket, alternativ über `mcp_server`).** Ein Adapter erschließt Smart Home,
   Klimasensoren, Anwesenheit und über die HA-Companion-App auch einen Teil der Handy- und Gesundheitsdaten.
   Er läuft auf sz neben HA. Testet den Fall „hohe Frequenz → Bündeln“ und später `act` (Licht schalten als
   Vorschlag).
3. **Matrix lesend (Stufe 0, `matrix-sdk`).** Der Besitzer nutzt Matrix schon (Raum „Homelab“). Über
   mautrix-Bridges kommen auch andere Messenger herein. Testet E2EE-Schlüssel im Host und personenbezogene Daten
   (wer darf was sehen, Phase WIR).

Parallel und klein: **Symbiot-MCP-Server** mit `search` und `propose_*`. Gesundheit und Wearables erst danach:
Android Health Connect bräuchte ein Tauri-Mobile-Plugin, Apple HealthKit einen Mac. *Beides ungeprüft.*

### c8. Risiken und offene Fragen

- **Component Model in Bewegung:** WASI 0.3 ist erst seit Juni 2026 stabil. „Component Model 1.0“ steht noch
  aus. Mit Brüchen in `wit-bindgen` und den WASI-Schnittstellen ist zu rechnen, deshalb `symbiot:adapter`
  selbst versionieren.
- **Belegflut gegen „der Mensch entscheidet“:** Ein Kalender liefert hunderte Belege. Wer jeden einzeln
  bestätigen muss, gibt auf. Der Vorschlag hier (Grant erlaubt Beobachten, nur Übernahme und Handeln brauchen
  einen Klick) muss der Besitzer ausdrücklich so wollen. Offene Frage: Soll es *Regeln* geben („Termine aus
  Kalender X immer mit Ziel Y verknüpfen“)? Das wäre Automatik, also bewusst entscheiden.
- **Kettenwachstum:** Sensoren und Smart Home erzeugen viele Einträge. Nötig sind Bündelung, Aufbewahrungsregeln
  (Rohpayloads löschen, Hash behalten) und eventuell Snapshots. Das ist im Kern noch nicht vorgesehen.
- **Gehostete Adapter sehen Klartext.** Für sensible Quellen gilt: nur lokal oder auf sz. Das sollte als Feld im
  Grant stehen und in der UI sichtbar sein.
- **Schlüssel auf gehosteten Orten:** Wenn der Adapter-Schlüssel auf einem fremden Host liegt, kann der Betreiber
  in seinem Namen signieren. Der Grant begrenzt den Schaden (Scope, Ablaufdatum), verhindert ihn aber nicht.
- **Prompt-Injection über Adapterdaten:** Mails und Nachrichten sind Angreifer-Text. Die KI darf sie lesen, ihr
  Ergebnis ist aber nur ein Vorschlag. Der Masken-Filter für Claude muss auch Adapterdaten erfassen.
- **Rust-Bibliotheken für CalDAV und JMAP sind dünn.** Eventuell braucht es einen eigenen kleinen Client im
  Gast (ICS-Parser, HTTP REPORT).
- **Lizenzen:** Nango (ELv2, Self-Hosting teils Enterprise) und n8n (Sustainable Use License) sind nicht frei im
  OSI-Sinn. Nicht in den Kern übernehmen.
- **Offen:** Braucht es eine Registry oder Signatur für *Adapter-Autoren* (wer hat die Komponente gebaut)? Für
  den Einzelnutzer reicht der Hash. Für Phase ALLE wäre ein SCITT-artiges Transparenzlog denkbar.

---

## (d) Quellen

**WebAssembly, Plugin-Laufzeiten**
- WASI 0.3 Ankündigung: https://bytecodealliance.org/articles/WASI-0.3 · Release-Seite: https://wasi.dev/releases/wasi-p3
- Migration 0.2 → 0.3: https://component-model.bytecodealliance.org/design/migrating-to-p3.html
- Wasmtime Plattform-Support, Pulley, no_std: https://docs.rs/crate/wasmtime-cli/45.0.0/source/docs/stability-platform-support.md
- Wasmtime auf Pico 2 (Zulip, SIG-Embedded): https://bytecodealliance.github.io/zulip-archive/stream/438936-SIG-Embedded/topic/Running.20Wasmtime.20on.20Pico.202.3A.20Overcoming.20Memory.20issues.html
- Extism Rust-SDK: https://docs.rs/crate/extism/latest · Konzepte: https://extism.org/docs/category/concepts
- Zed Extensions: https://zed.dev/docs/extensions/developing-extensions.html · Capabilities (Spiegel der Doku): https://git.secluded.site/zed/blob/v0.217.1/docs/src/extensions/capabilities.md?raw=1
- Lapce (WASI-Plugins): https://github.com/lapce/lapce
- Spin Capabilities (Rust-Doku): https://spinframework.github.io/rust-docs/spin/main/src/spin_capabilities/lib.rs.html
- Envoy Dynamic Modules: https://www.envoyproxy.io/docs/envoy/latest/intro/arch_overview/advanced/dynamic_modules
- Tauri 2 Capabilities/ACL: https://v2.tauri.app/reference/acl/capability · https://v2.tauri.app/security/capabilities/
- Obsidian Plugin-Sicherheit: https://obsidian.md/help/plugin-security
- VS Code Erweiterungen ohne Sandbox *(sekundär)*: https://www.sonarsource.com/blog/visual-studio-code-security-deep-dive-into-your-favorite-editor/
- Logseq iframe-Lücke CVE-2026-47901 *(sekundär, Fix ungeprüft)*: https://cve.cert.hr/cve/CVE-2026-47901

**Integrationsprotokolle**
- MCP Spec 2026-07-28 (Release): https://github.com/modelcontextprotocol/specification/releases/tag/2026-07-28 · Changelog: https://modelcontextprotocol.io/specification/2026-07-28/changelog
- MCP Go-SDK Release-Notes (veraltete Roots/Sampling/Logging): https://github.com/modelcontextprotocol/go-sdk/releases
- MCP 2025-11-25 Änderungen *(sekundär)*: https://workos.com/blog/mcp-2025-11-25-spec-update
- MCP Apps (SEP-1865): https://modelcontextprotocol.io/seps/1865-mcp-apps-interactive-user-interfaces-for-mcp · Spec und SDK: https://github.com/modelcontextprotocol/ext-apps
- MCP-Sicherheit, Tool Poisoning *(sekundär)*: https://www.cyberark.com/resources/all-blog-posts/poison-everywhere-no-output-from-your-mcp-server-is-safe · https://www.backslash.security/blog/hundreds-of-mcp-servers-vulnerable-to-abuse
- Home Assistant MCP-Server: https://www.home-assistant.io/integrations/mcp_server · Integration-Manifest (Spiegel der Entwicklerdoku): https://git.sudo.is/home-assistant/developers.home-assistant/src/branch/bump-waiton/docs/creating_integration_manifest.md
- rs-matter: https://docs.rs/crate/rs-matter/0.3.0 · Embassy: https://github.com/oddlama/rs-matter-embassy
- mautrix bridgev2 *(Blog des Autors)*: https://mau.fi/blog/2024-h1-mautrix-updates/
- JMAP Specs: https://jmap.io/spec.html · IETF JMAP WG: https://datatracker.ietf.org/wg/jmap/about/ · calext: https://ietf.potaroo.net/html/ids-wg-calext.html
- Airbyte-Protokoll: https://docs.airbyte.com/platform/understanding-airbyte/airbyte-protocol
- Nango Self-Hosting *(sekundär, widersprüchlich)*: https://railway.com/deploy/nango · https://getknit.dev/blog/nango-review-evaluation-integration-platform
- W3C Linked Web Storage (Solid): https://www.w3.org/groups/wg/lws/charters/active/ · https://www.w3.org/standards/history/lws10-core/

**Herkunft und Einwilligung**
- VC Data Model 2.0: https://www.w3.org/TR/vc-data-model-2.0/ · Rec.-Datum *(sekundär)*: https://startwithidentity.com/blog/2025-05-15-w3c-publishes-verifiable-credentials-data-model-2-0-as-a-recommendatio/
- Data Integrity EdDSA: https://www.w3.org/TR/vc-di-eddsa/
- SCITT Architektur RFC 9943: https://www.rfc-editor.org/rfc/rfc9943.html
- C2PA Rust-SDK: https://github.com/contentauth/c2pa-rs
- Data Transfer Initiative, Jahresbericht 2025: https://dtinit.org/blog/2026/01/27/annual-report-2025 · Stand 07/2026: https://dtinit.org/blog/2026/07/21/midstride-check-in

**Local-first**
- Keyhive Lab-Notizen (Ink & Switch): https://www.inkandswitch.com/keyhive/notebook/
- Iroh 1.0: https://iroh.computer/blog/road-to-1-0 · https://docs.rs/crate/iroh/1.0.0/source/README.md
- Anytype any-sync: https://github.com/anyproto/any-sync · https://github.com/anyproto/tech-docs
