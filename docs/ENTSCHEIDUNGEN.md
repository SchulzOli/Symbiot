# Entscheidungen

Stand: 2026-10-30. Jede Entscheidung hat einen Status: **entschieden** (vom Besitzer), **Vorschlag** (wartet auf
den Besitzer) oder **offen**. Was hier entschieden ist, gilt vor PLAN.md und FEATURES.md, bis diese nachgezogen sind.

## E1 — Symbiot und Connect sind ein System · *entschieden*

Ein Projekt, ein System. Connect (Austausch mit Menschen, Maschinen, Daten, dem eigenen Gedächtnis) ist
kein eigenes Produkt, sondern der Teil von Symbiot, der über einen einzelnen Menschen hinausgeht – bisher
„Phase WIR / ALLE“.

**Der Mensch entscheidet, wo welche Schicht liegt:** auf dem Gerät, auf dem eigenen Server oder gehostet.
Wir geben nur Empfehlungen. Daraus folgt für die Architektur:

- Jede Schicht (Kette und Speicher, KI, Sync-Knoten, Adapter, Oberfläche) muss auf jedem dieser Orte laufen
  können. Kein Teil darf voraussetzen, dass ein anderer am selben Ort läuft.
- Die Empfehlung ist Teil der Einrichtung und ist veränderbar, z. B. „Kette auf dem Gerät, Kopie auf dem
  eigenen Server, KI lokal; Claude nur über den Masken-Filter“.

## E2 — Löschen: Löschmarker · *entschieden* · Ergänzung als *Vorschlag*

Löschen heißt: ein neuer, signierter Eintrag `DeleteNode` bzw. `Unlink` (Löschmarker). Die Kette bleibt
lückenlos prüfbar, die Projektion zeigt den Knoten bzw. die Kante nicht mehr.

**Vorschlag zur Ergänzung:** Ein Löschmarker allein entfernt den Inhalt nicht – Titel und Text stehen weiter
im früheren Eintrag. Damit „löschbar“ wirklich löscht, schlagen wir vor, das Kettenformat **vor M4** so zu
ändern, dass die Signatur nicht über den Inhalt selbst läuft, sondern über dessen Hash:

```
hash = sha256(prev ‖ postcard(kopf) ‖ sha256(inhalt))
```

Dann darf ein Gerät nach einem Löschmarker den Inhalt früherer Einträge wegräumen (nur der Inhalts-Hash
bleibt), und `verify()` bestätigt die Kette trotzdem. Geräte, die eine Kopie haben, räumen beim Sync ebenso
auf. Ein fremdes Gerät kann dazu nicht gezwungen werden – das gilt für jede Löschung in einem verteilten
System und muss in der Oberfläche ehrlich so stehen.

## E3 — „Löschbar“ und „exportierbar“ entscheidet der Mensch, pro Knoten · *entschieden*

- Bei jedem neuen Knoten lässt sich festlegen, ob er **löschbar** und ob er **exportierbar** ist.
  Standard: **beides ja**.
- Das ist eine **technische Unterscheidung ohne Einfluss auf das Datenmodell**: Knoten haben überall
  dieselbe Form, damit Knoten und Ketten zwischen Geräten und Installationen kompatibel bleiben.

**Vorschlag zur Umsetzung:** Die beiden Schalter stehen nicht im Knoten, sondern als **Richtlinie** in einem
eigenen Eintrag (`SetPolicy { node, deletable, exportable }`), den die Projektion auswertet:

| Richtlinie | Wirkung |
|---|---|
| löschbar | Löschmarker + Inhalt wird weggeräumt (E2) |
| nicht löschbar | Löschmarker blendet nur aus, Inhalt bleibt in der Kette (z. B. für Belege, die bewiesen bleiben müssen) |
| exportierbar | erscheint in Export und Sync zu Geräten anderer Menschen |
| nicht exportierbar | bleibt auf den eigenen Geräten, verlässt sie nie (auch nicht zur gehosteten KI) |

Offen: Darf eine Richtlinie später gelockert werden (nicht löschbar → löschbar)? Verschärfen sollte immer gehen.

## E4 — Was ist sensibel? · *entschieden*

- **Die KI erkennt** möglicherweise sensible Inhalte und **schlägt eine Regel vor** (z. B. „Gesundheitsdaten
  sind sensibel“). **Zur Regel wird es erst, wenn der Mensch zustimmt.** Angenommene Regeln sind Einträge in
  der Kette und werden danach deterministisch angewandt – die KI entscheidet nicht selbst.
- **Markieren von Hand** ist immer möglich, mit einem Klick, an jedem Knoten. Eine Markierung von Hand gilt
  vor jeder Regel.
- Sensible Knoten verlassen das Heimnetz nie, auch nicht zur gehosteten KI.

## E5 — Schlüsselverlust und Wiederherstellung · *Vorschlag*

Ja, wir brauchen ein Konzept – aber ein kleines. Backups und Umzug auf ein neues Gerät sind der richtige
Ansatz. Wichtig: Der **Geräteschlüssel wird dabei nicht mit umgezogen.**

- **Personen-Schlüssel:** Der Mensch hat einen eigenen Schlüssel (angezeigt als Wiederherstellungs-Wörter,
  ausgedruckt oder im Passwort-Manager). Er unterschreibt die Geräteschlüssel: „Dieses Gerät gehört mir.“
- **Neues Gerät:** bekommt einen **neuen** Geräteschlüssel, wird vom Personen-Schlüssel bestätigt und lädt
  das Backup bzw. synchronisiert die Ketten.
- **Verlorenes oder gestohlenes Gerät:** wird mit dem Personen-Schlüssel **widerrufen**. Einträge, die es
  nach dem Widerruf signiert, werden abgelehnt. Würde man den alten Schlüssel weiterverwenden, könnte ein Dieb
  im Namen des Menschen schreiben.
- **Backup** = alle Ketten, verschlüsselt (E6). Daraus lässt sich alles andere (Knoten, Kanten, Suche)
  wieder aufbauen.

| Wiederherstellbar | Nicht wiederherstellbar (bewusst) |
|---|---|
| alle Ketten, damit alle Knoten, Kanten, Verläufe, Richtlinien und Regeln | der alte Geräteschlüssel |
| Gerätebestätigungen und Widerrufe | Inhalte, die nach E2 weggeräumt wurden |
| | Knoten, die „nicht exportierbar“ sind, wenn das Backup ein anderes Gerät verlässt – **offen:** zählt das eigene Backup als Export? |

## E6 — Verschlüsselung des Speichers · *Vorschlag*

Ja, SQLite ist heute der einzige Speicher: Die Kette ist dort die Wahrheit, Knoten und Kanten sind daraus
abgeleitet. **Empfehlung: SQLite behalten und verschlüsseln, keine neue Datenbank.**

- **SQLCipher** (in `rusqlite` als Feature `bundled-sqlcipher`): die ganze Datei mit AES-256 verschlüsselt,
  auf Windows, macOS, Linux, Android und iOS erprobt, kein Umbau der Abfragen nötig.
- Der Schlüssel liegt im Schlüsselbund des Betriebssystems (Windows Credential Manager, macOS/iOS Keychain,
  Android Keystore) oder wird aus einer Passphrase abgeleitet; auf dem Server als Docker-Secret.
- Backups und Sync sind zusätzlich Ende-zu-Ende verschlüsselt, damit ein gehosteter Ort (E1) nur Chiffretext
  sieht.
- Der ESP32 hat keine Datenbank; er signiert nur und schickt weiter.

Alternative, falls SQLCipher in Tauri mobil Probleme macht: Inhalte vor dem Speichern einzeln verschlüsseln
(z. B. XChaCha20-Poly1305). Mehr Arbeit, aber unabhängig von der Datenbank.

## E7 — Adapter und Plugins · *offen, Recherche läuft*

Wie kommen Kalender, Nachrichten, Geräte im Haus, Gesundheitsdaten und andere Apps herein, und wie sehen
Anzeige-Plugins aus? Wir wollen auf dem Stand der Technik sein. Recherche: `docs/RECHERCHE-ADAPTER.md`.

## E8 — Zurückgestellt · *entschieden*

- **Neue Knotenarten** wie „Absicht“ oder „Gegenseite“: erst einmal nicht.
- **iOS:** später.

## E9 — KI für M3 · *entschieden*

LiteLLM auf heimserver mit `qwen3:8b` ist erreichbar, der Masken-Filter für Claude existiert. M3 kann beginnen.
