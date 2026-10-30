# Erste Features

Reihenfolge = Bauweise. F1 und F2 bilden zusammen das Walking Skeleton (M1).

## F1 — Erfassen *(M1)*
Ich halte fest, was mich beschäftigt.
- Knoten anlegen mit Art (`Ziel`, `Spannung`, `Reflexion`, `Beleg`, `Person`, `Wert`), Titel und Text.
- Liste aller Knoten, filterbar nach Art.
- **Fertig, wenn:** Ein Knoten überlebt einen Neustart der App. Er erscheint in der Desktop-App und in der Web-Version auf heimserver (getrennte Daten bis M4).

## F2 — Beweisbar *(M1)*
Nichts wird heimlich verändert.
- Jede Änderung ist ein signierter Eintrag in der Kette des Geräts: `hash = sha256(prev ‖ postcard(entry))` plus Ed25519-Signatur.
- Knopf „Kette prüfen“ zeigt `ok · N Einträge` oder die genaue Bruchstelle.
- Bearbeiten heißt neuer Eintrag. Die alte Version bleibt sichtbar.
- **Fertig, wenn:** Ein Property-Test beweist, dass jedes veränderte Byte, jeder vertauschte und jeder fehlende Eintrag erkannt wird.

## F3 — Verknüpfen *(M2)*
Zusammenhänge sichtbar machen.
- Kanten: `stützt`, `widerspricht`, `bezieht sich auf`, `abgeleitet von`.
- Graph-Ansicht (SVG in Leptos), Klick auf Knoten zeigt seine Herkunft im Ledger.
- **Fertig, wenn:** Ein E2E-Test legt Ziel und Beleg an, verbindet sie und findet die Kante in der Ansicht.

## F4 — Mitdenken *(M3)*
Die KI schlägt den nächsten Schritt vor, ich entscheide.
- Für ein Ziel: „Nächsten Schritt vorschlagen“ → Anfrage über LiteLLM (Standard: lokales `qwen3:8b`).
- Der Vorschlag nennt seine **Quellen** (Knoten-IDs). Ein Vorschlag ohne Quellen wird verworfen.
- Annehmen oder Ablehnen wird als Eintrag ins Ledger geschrieben. Angenommen = neuer Knoten.
- **Fertig, wenn:** Kein Vorschlag ändert Daten ohne deinen Klick (Test). Bei sensiblen Knoten verlässt nichts das Heimnetz.

## F5 — Synchron *(M4)*
Ein Gedächtnis auf allen Geräten.
- Geräte melden sich mit ihrem öffentlichen Schlüssel an heimserver an (Freigabeliste).
- Austausch: „Gib mir alles nach Eintrag N von Kette X“. Fremde Einträge werden vor dem Speichern geprüft.
- **Fertig, wenn:** Offline am PC angelegte Knoten nach dem Wiederverbinden auf heimserver und auf dem zweiten Gerät erscheinen. Eine manipulierte Kette wird abgelehnt.

## F6 — Überall *(M5)*
Symbiot auf dem Handy.
- Android-Build aus derselben Codebasis, im Release als signiertes APK.
- **Fertig, wenn:** Ein Ziel am Handy angelegt wird und am PC erscheint.

## F7 — Sensor *(M6)*
Belege, die kein Mensch eintippen muss.
- ESP32-C3 mit `symbiot-core`, eigener Schlüssel, sendet signierte Messwerte an heimserver.
- **Fertig, wenn:** Ein Messwert als Beleg-Knoten erscheint und `Kette prüfen` die Kette des Sensors bestätigt.

## Später (Phase WIR / ALLE)
Personen-Graph und Vertrauensräume (GraphFamiliy, TrustInHumanity) · Einwilligungen pro Knoten und Person · Konsens-Werkzeug (IchWillNichtStreiten) · gemeinsamer Ledger (AGORA).
