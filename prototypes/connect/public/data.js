'use strict';

/*
 * Die Idee hinter Connect als Graph: Knoten (Akteure, Bausteine, Kanäle, Zwecke, Werkzeuge)
 * und Kanten (wer mit wem worüber verbunden ist). Szenarien sind Wege durch diesen Graphen,
 * die Schritt für Schritt zeigen, wie ein Austausch abläuft.
 *
 * ring bestimmt den Abstand zur Mitte im 3D-Layout: 0 = Kern, 1 = innen, 3 = außen.
 */

window.CONNECT_DATA = {
  groups: {
    kern: { label: 'Kern', color: '#ffffff', ring: 0 },
    akteur: { label: 'Akteure', color: '#ffb547', ring: 1 },
    baustein: { label: 'Bausteine', color: '#5ad1ff', ring: 2 },
    kanal: { label: 'Kanäle & Adapter', color: '#9b8cff', ring: 3 },
    zweck: { label: 'Wozu', color: '#5cf2a6', ring: 3 },
    werkzeug: { label: 'Vorhandene Werkzeuge', color: '#ff6fa8', ring: 3.5 },
  },

  nodes: [
    {
      id: 'connect', group: 'kern', label: 'Connect',
      desc: 'Die Interoperabilitätsplattform. Sie verbindet alles mit allem: Menschen, Maschinen, Daten '
        + 'und das eigene Gedächtnis. Nicht ein weiteres Silo, sondern die Schicht dazwischen – sie '
        + 'versteht, was der eine will und was der andere braucht, und übersetzt dazwischen.',
    },

    // Akteure
    {
      id: 'ich', group: 'akteur', label: 'Ich',
      desc: 'Der Mensch im Mittelpunkt. Er hat Absichten, Wissen, Fragen und Daten – und behält die '
        + 'Hoheit darüber, was er teilt und mit wem.',
    },
    {
      id: 'andere', group: 'akteur', label: 'Andere Menschen',
      desc: 'Freunde, Kolleginnen, Fremde mit passendem Wissen. Austausch heißt: Was ich will, trifft auf '
        + 'das, was der andere will – und beide gewinnen.',
    },
    {
      id: 'maschinen', group: 'akteur', label: 'Maschinen & Agenten',
      desc: 'Programme, KI-Agenten, Dienste und Geräte. Sie handeln im Auftrag – aber Connect sorgt dafür, '
        + 'dass sie fragen, bevor sie etwas Wichtiges tun.',
    },
    {
      id: 'daten', group: 'akteur', label: 'Daten & Welt',
      desc: 'Messwerte, Dokumente, offene Datensätze, das Web. Daten sind selbst ein Gegenüber: Man kann sie '
        + 'befragen, verknüpfen und daraus lernen.',
    },
    {
      id: 'selbst', group: 'akteur', label: 'Mein künftiges Ich',
      desc: 'Der Austausch mit sich selbst über die Zeit: Was ich heute denke, lerne und entscheide, steht mir '
        + 'morgen wieder zur Verfügung. Erinnerung wird zum Gesprächspartner.',
    },

    // Bausteine der Plattform
    {
      id: 'absicht', group: 'baustein', label: 'Absicht',
      desc: '„Das ist, was ich will.“ Ein Wunsch, eine Frage oder ein Angebot wird so beschrieben, dass '
        + 'Menschen und Maschinen ihn verstehen – mit Bedeutung statt nur mit Schlagworten.',
    },
    {
      id: 'angebot', group: 'baustein', label: 'Gegenseite',
      desc: '„Das ist, was der andere will.“ Bedarf und Angebot der anderen Seite, in derselben Sprache '
        + 'beschrieben wie die eigene Absicht – erst dadurch werden beide vergleichbar.',
    },
    {
      id: 'vermittlung', group: 'baustein', label: 'Vermittlung',
      desc: 'Findet, wer oder was zueinander passt: über Bedeutungsähnlichkeit, Kontext und Vertrauen. '
        + 'Schlägt Verbindungen vor – entscheidet aber nicht über den Kopf der Beteiligten hinweg.',
    },
    {
      id: 'uebersetzung', group: 'baustein', label: 'Übersetzung',
      desc: 'Das Herz der Interoperabilität: wandelt Formate, Schemata, Sprachen und Protokolle ineinander '
        + 'um. „Egal mit was“ – jede neue Quelle braucht nur einen Adapter, nicht n neue Verbindungen.',
    },
    {
      id: 'identitaet', group: 'baustein', label: 'Identität',
      desc: 'Wer bin ich, wer ist das Gegenüber, ist es ein Mensch oder eine Maschine? Selbstbestimmte '
        + 'Identitäten statt Konten bei jedem Anbieter.',
    },
    {
      id: 'einwilligung', group: 'baustein', label: 'Einwilligung',
      desc: 'Jeder Datenfluss braucht eine Erlaubnis: zweckgebunden, widerrufbar, verständlich. Maschinen '
        + 'schlagen vor, Menschen bestätigen.',
    },
    {
      id: 'vertrauen', group: 'baustein', label: 'Vertrauen & Nachweis',
      desc: 'Jeder Austausch hinterlässt eine nachvollziehbare Spur: wer hat was wann wozu geteilt. '
        + 'Regeln sind festgelegt und deterministisch – nie vom Modell erfunden.',
    },
    {
      id: 'gedaechtnis', group: 'baustein', label: 'Gedächtnis',
      desc: 'Ein persönlicher Wissensgraph: Gespräche, Dokumente, Erkenntnisse und Beziehungen, verknüpft '
        + 'und durchsuchbar. Gehört dem Menschen, liegt bei ihm, wandert mit ihm.',
    },
    {
      id: 'protokolle', group: 'baustein', label: 'Offene Protokolle',
      desc: 'Gemeinsame Sprache der Plattform: offene Schnittstellen statt geschlossener Ökosysteme, damit '
        + 'jeder anschließen kann – auch ohne Connect selbst.',
    },

    // Kanäle & Adapter
    {
      id: 'sprache', group: 'kanal', label: 'Sprache',
      desc: 'Sprechen und zuhören – die natürlichste Schnittstelle zwischen Mensch und Maschine.',
    },
    {
      id: 'chat', group: 'kanal', label: 'Text & Nachrichten',
      desc: 'Chat, E-Mail, Messenger: bestehende Kanäle werden angeschlossen statt ersetzt.',
    },
    {
      id: 'dokumente', group: 'kanal', label: 'Dokumente',
      desc: 'PDFs, Notizen, Tabellen, Fotos – Wissen in Dateien wird lesbar und verknüpfbar.',
    },
    {
      id: 'apis', group: 'kanal', label: 'APIs & Dienste',
      desc: 'Programmierschnittstellen von Apps und Diensten, angebunden über Adapter.',
    },
    {
      id: 'sensoren', group: 'kanal', label: 'Sensoren & Geräte',
      desc: 'Uhr, Telefon, Haus, Auto: Messwerte aus der physischen Welt.',
    },
    {
      id: 'mcp', group: 'kanal', label: 'Agenten-Werkzeuge (MCP)',
      desc: 'Werkzeuge, die KI-Agenten aufrufen können. Jede Fähigkeit von Connect ist auch als Werkzeug '
        + 'für Agenten verfügbar.',
    },
    {
      id: 'web', group: 'kanal', label: 'Web & offene Daten',
      desc: 'Öffentliches Wissen: Websites, Lexika, Datenportale, Forschung.',
    },

    // Zwecke
    {
      id: 'verstehen', group: 'zweck', label: 'Die Welt verstehen',
      desc: 'Mehr über die Welt erfahren: Fragen stellen, Quellen verbinden, Zusammenhänge sehen.',
    },
    {
      id: 'wissen', group: 'zweck', label: 'Wissen speichern',
      desc: 'Was einmal gelernt wurde, geht nicht verloren – es wird abgelegt, verknüpft und wiedergefunden.',
    },
    {
      id: 'erinnern', group: 'zweck', label: 'Gedächtnis besitzen',
      desc: 'Das eigene Gedächtnis gehört einem selbst – nicht einer Plattform. Exportierbar, löschbar, privat.',
    },
    {
      id: 'zusammenarbeit', group: 'zweck', label: 'Zusammenarbeiten',
      desc: 'Menschen finden sich über das, was sie wollen und können, und arbeiten gemeinsam – unterstützt '
        + 'von Maschinen.',
    },

    // Vorhandene Werkzeuge aus oli-tech-tools als erste Bausteine
    {
      id: 't-semantic', group: 'werkzeug', label: 'Semantic Similarity',
      desc: 'Vorhandenes Werkzeug (oli-tech-tools): Bedeutungsähnlichkeit per Embeddings, lokal. '
        + 'Grundlage für die Vermittlung von Absicht und Gegenseite.',
    },
    {
      id: 't-aci', group: 'werkzeug', label: 'ACI',
      desc: 'Vorhandenes Werkzeug (oli-tech-tools): Agenten fragen Menschen über Formulare nach Entscheidungen. '
        + 'Die Brücke Maschine → Mensch für Einwilligungen.',
    },
    {
      id: 't-avatar', group: 'werkzeug', label: 'Open Avatar',
      desc: 'Vorhandenes Werkzeug (oli-tech-tools): lokale Sprach-Schnittstelle mit Avatar (STT, LLM, TTS). '
        + 'Der Sprachkanal von Connect.',
    },
    {
      id: 't-docs', group: 'werkzeug', label: 'OnePDF & TeXify',
      desc: 'Vorhandene Werkzeuge (oli-tech-tools): Dokumente zusammenführen und erzeugen. '
        + 'Adapter für den Kanal „Dokumente“.',
    },
    {
      id: 't-dr', group: 'werkzeug', label: 'DR-Cockpit',
      desc: 'Vorhandenes Werkzeug (oli-tech-tools): Agenten schreiben nie direkt, sondern machen Vorschläge '
        + 'mit festen Regeln und Audit. Vorbild für Vertrauen & Nachweis.',
    },
  ],

  // Symbol je Knoten (Emoji, kommt aus den Systemschriften – nichts wird nachgeladen)
  icons: {
    connect: '✳️', ich: '🙂', andere: '👥', maschinen: '🤖', daten: '🌍', selbst: '🪞',
    absicht: '🎯', angebot: '🤲', vermittlung: '🔗', uebersetzung: '🔄', identitaet: '🪪',
    einwilligung: '✅', vertrauen: '🛡️', gedaechtnis: '🧠', protokolle: '📜',
    sprache: '🎙️', chat: '💬', dokumente: '📄', apis: '🔌', sensoren: '⌚', mcp: '🧰', web: '🌐',
    verstehen: '🔭', wissen: '📚', erinnern: '🗝️', zusammenarbeit: '🤝',
    't-semantic': '🧭', 't-aci': '🙋', 't-avatar': '🗣️', 't-docs': '🗂️', 't-dr': '🧾',
  },

  // Medien, die als Pakete über die Kanten wandern: Sprache, Text, Bild, Dokument, Messwert, Erinnerung …
  media: ['🗣️', '💬', '🖼️', '📄', '📊', '💭', '🎵', '📍', '🔑'],

  // Stufen: vom einfachen Gedanken zum vollständigen Graphen
  levels: [
    {
      label: '1 · Die Idee',
      groups: ['kern', 'akteur'],
      text: 'Ich ⇄ Connect ⇄ alles andere: Menschen, Maschinen, Daten – und mein künftiges Ich.',
    },
    {
      label: '2 · Die Plattform',
      groups: ['kern', 'akteur', 'baustein'],
      text: 'Was dafür nötig ist: Absicht, Gegenseite, Vermittlung, Übersetzung, Identität, Einwilligung, '
        + 'Vertrauen, Gedächtnis.',
    },
    {
      label: '3 · Das Ökosystem',
      groups: ['kern', 'akteur', 'baustein', 'kanal', 'zweck', 'werkzeug'],
      text: 'Egal mit was: Kanäle und Adapter, die Zwecke dahinter – und die Werkzeuge, die es schon gibt.',
    },
  ],

  // [von, nach, Beschriftung]
  edges: [
    ['connect', 'ich', 'verbindet'],
    ['connect', 'andere', 'verbindet'],
    ['connect', 'maschinen', 'verbindet'],
    ['connect', 'daten', 'verbindet'],
    ['connect', 'selbst', 'verbindet'],

    ['connect', 'absicht', 'besteht aus'],
    ['connect', 'vermittlung', 'besteht aus'],
    ['connect', 'uebersetzung', 'besteht aus'],
    ['connect', 'identitaet', 'besteht aus'],
    ['connect', 'einwilligung', 'besteht aus'],
    ['connect', 'vertrauen', 'besteht aus'],
    ['connect', 'gedaechtnis', 'besteht aus'],
    ['connect', 'protokolle', 'besteht aus'],

    ['ich', 'absicht', 'äußert'],
    ['ich', 'identitaet', 'hat'],
    ['ich', 'gedaechtnis', 'besitzt'],
    ['ich', 'einwilligung', 'erteilt'],
    ['ich', 'sprache', 'spricht'],
    ['ich', 'chat', 'schreibt'],
    ['ich', 'andere', 'tauscht aus'],
    ['ich', 'maschinen', 'beauftragt'],
    ['ich', 'daten', 'befragt'],
    ['ich', 'selbst', 'erinnert'],

    ['andere', 'angebot', 'äußern'],
    ['andere', 'identitaet', 'haben'],
    ['andere', 'chat', 'schreiben'],
    ['maschinen', 'mcp', 'nutzen'],
    ['maschinen', 'apis', 'sprechen'],
    ['maschinen', 'protokolle', 'folgen'],
    ['maschinen', 'einwilligung', 'fragen nach'],
    ['daten', 'sensoren', 'kommen aus'],
    ['daten', 'dokumente', 'kommen aus'],
    ['daten', 'web', 'kommen aus'],
    ['daten', 'apis', 'kommen aus'],
    ['selbst', 'gedaechtnis', 'lebt in'],

    ['absicht', 'vermittlung', 'wird gesucht'],
    ['angebot', 'vermittlung', 'wird gefunden'],
    ['vermittlung', 'gedaechtnis', 'nutzt Kontext'],
    ['vermittlung', 'vertrauen', 'prüft'],
    ['uebersetzung', 'protokolle', 'spricht'],
    ['uebersetzung', 'sprache', 'adaptiert'],
    ['uebersetzung', 'chat', 'adaptiert'],
    ['uebersetzung', 'dokumente', 'adaptiert'],
    ['uebersetzung', 'apis', 'adaptiert'],
    ['uebersetzung', 'sensoren', 'adaptiert'],
    ['uebersetzung', 'mcp', 'adaptiert'],
    ['uebersetzung', 'web', 'adaptiert'],
    ['uebersetzung', 'gedaechtnis', 'schreibt'],
    ['einwilligung', 'vertrauen', 'wird belegt'],
    ['identitaet', 'vertrauen', 'begründet'],
    ['identitaet', 'einwilligung', 'gehört zu'],

    ['gedaechtnis', 'wissen', 'ermöglicht'],
    ['gedaechtnis', 'erinnern', 'ermöglicht'],
    ['vermittlung', 'zusammenarbeit', 'ermöglicht'],
    ['vermittlung', 'verstehen', 'ermöglicht'],
    ['web', 'verstehen', 'speist'],
    ['andere', 'zusammenarbeit', 'gestalten'],

    ['t-semantic', 'vermittlung', 'treibt'],
    ['t-semantic', 'gedaechtnis', 'durchsucht'],
    ['t-aci', 'einwilligung', 'fragt Menschen'],
    ['t-aci', 'mcp', 'als Werkzeug'],
    ['t-avatar', 'sprache', 'spricht'],
    ['t-docs', 'dokumente', 'verarbeitet'],
    ['t-dr', 'vertrauen', 'Vorbild für'],
    ['t-dr', 'einwilligung', 'Vorschläge'],
  ],

  // Szenarien: Wege durch den Graphen, Schritt für Schritt erzählt
  scenarios: [
    {
      id: 'mensch-mensch', label: 'Mensch ↔ Mensch',
      steps: [
        ['ich', 'Ich habe eine Frage: „Wer kennt sich mit Solaranlagen auf Altbauten aus?“'],
        ['absicht', 'Connect hält fest, was ich will – mit Bedeutung, nicht nur mit Schlagworten.'],
        ['vermittlung', 'Die Vermittlung sucht nach Menschen, deren Angebot dazu passt.'],
        ['angebot', 'Jemand hat geschrieben: „Ich habe gerade selbst eine gebaut und teile gern.“'],
        ['einwilligung', 'Beide sehen den Vorschlag und stimmen zu, bevor Kontaktdaten fließen.'],
        ['andere', 'Verbindung hergestellt: Was ich will, trifft auf das, was der andere will.'],
      ],
    },
    {
      id: 'mensch-maschine', label: 'Mensch ↔ Maschine',
      steps: [
        ['ich', '„Plane mir die Reise nach Lissabon.“'],
        ['sprache', 'Gesprochen, nicht getippt – über die Sprach-Schnittstelle.'],
        ['uebersetzung', 'Die Absicht wird in die Sprache der Maschinen übersetzt.'],
        ['maschinen', 'Ein Agent sucht Verbindungen und Unterkünfte über angeschlossene Dienste.'],
        ['einwilligung', 'Vor dem Buchen fragt der Agent nach – die Maschine schlägt vor, der Mensch entscheidet.'],
        ['ich', 'Ich bestätige. Alles ist im Nachweis festgehalten.'],
      ],
    },
    {
      id: 'mensch-daten', label: 'Mensch ↔ Daten',
      steps: [
        ['ich', '„Schlafe ich schlechter, wenn es draußen warm ist?“'],
        ['daten', 'Die Antwort steckt in Daten, die schon existieren.'],
        ['sensoren', 'Schlafdaten von der Uhr …'],
        ['uebersetzung', '… und Wetterdaten aus dem Web werden in ein gemeinsames Format gebracht.'],
        ['vermittlung', 'Connect verknüpft beide Quellen über die Zeit.'],
        ['verstehen', 'Ein Zusammenhang wird sichtbar – ich habe etwas über meine Welt gelernt.'],
      ],
    },
    {
      id: 'mensch-selbst', label: 'Mensch ↔ sich selbst',
      steps: [
        ['ich', 'Eine Idee unter der Dusche, ein Gespräch, ein gelesenes Buch.'],
        ['chat', 'Kurz festgehalten – als Notiz, Sprachnachricht oder Foto.'],
        ['uebersetzung', 'Connect macht daraus verknüpfbares Wissen.'],
        ['gedaechtnis', 'Es landet in meinem persönlichen Gedächtnis – bei mir, nicht in einer fremden Cloud.'],
        ['selbst', 'Monate später fragt mein künftiges Ich: „Was hatte ich mir damals überlegt?“'],
        ['erinnern', 'Und bekommt die Antwort. Ich besitze mein Gedächtnis.'],
      ],
    },
    {
      // Alltags-Szenario mit Steckbrief je Schritt: Technik, Server, Eigentümer, Daten und Anzeige
      id: 'alltag', label: '▶ Alltag: Abendessen planen', featured: true, stepMs: 7000,
      steps: [
        ['ich', '18:05, auf dem Heimweg: „Was kochen wir heute Abend?“', {
          stack: 'Open Avatar: Spracherkennung lokal (Whisper, Rust/axum), WebAudio',
          server: 'Auf dem Handy · offline · Audio wird nicht gespeichert',
          owner: 'Ich',
          data: ['Sprachbefehl als Text', '+ Kalender: wer ist heute zu Hause?'],
          plugin: { name: 'Sprach-Blase', icon: '🗣️', lines: ['„Was kochen wir heute Abend?“', 'erkannt in 0,4 s · lokal'] },
        }],
        ['absicht', 'Connect macht daraus eine Absicht: Abendessen für 3, heute, unter 30 Minuten.', {
          stack: 'Absichts-Schema (JSON-LD, schema.org/Action), lokales Sprachmodell (Ollama)',
          server: 'Connect-Knoten zu Hause (Heimserver) · TLS 1.3 · 12 ms',
          owner: 'Ich',
          data: ['Absicht: Abendessen', '+ Kalender (3 Personen ab 19 Uhr)'],
          plugin: { name: 'Absichts-Karte', icon: '🎯', lines: ['Abendessen · 3 Personen', 'heute 19:00 · max. 30 min'] },
        }],
        ['gedaechtnis', 'Das Gedächtnis weiß: Lea isst kein Fleisch, Jonas mag keine Pilze, gestern gab es Pasta.', {
          stack: 'Persönlicher Wissensgraph (SQLite + Embeddings), Semantic Similarity',
          server: 'Heimserver · verschlüsselt (age) · Backup Hetzner Falkenstein (DE), Ende-zu-Ende',
          owner: 'Ich · den Bereich „Vorlieben“ teilt die Familie',
          data: ['Vorlieben & Allergien', 'Mahlzeiten-Verlauf', '+ Nährwert-Ziele (erweiterbar)'],
          plugin: { name: 'Erinnerungs-Zeitleiste', icon: '🧠', lines: ['gestern: Pasta 🍝', 'Lea: vegetarisch · Jonas: keine Pilze'] },
        }],
        ['sensoren', 'Der Kühlschrank meldet: Linsen, Tomaten, Paprika, eine halbe Zucchini.', {
          stack: 'Matter/Thread-Gerät → MQTT → Connect-Adapter',
          server: 'Gerät im Heimnetz · kein Cloud-Konto · nur Lesezugriff',
          owner: 'Haushalt',
          data: ['Vorrat & Haltbarkeit', '+ Kassenbon-Scan (Plugin nachrüstbar)'],
          plugin: { name: 'Vorrats-Liste', icon: '🧊', lines: ['Linsen · Tomaten · Paprika', 'Zucchini ½ – bis morgen verbrauchen'] },
        }],
        ['web', 'Connect fragt offene Rezept-Datenbanken – nur mit Zutaten, ohne Person.', {
          stack: 'Adapter für schema.org/Recipe über HTTP-API',
          server: 'Externer Dienst · anonyme Anfrage · Antwort in 180 ms',
          owner: 'Rezept-Anbieter (Lizenz CC BY)',
          data: ['Rezepte', 'Nährwerte', '+ Saisonkalender (erweiterbar)'],
          plugin: { name: 'Rezept-Vorschläge', icon: '📖', lines: ['42 Rezepte gefunden', 'gefiltert nach Vorrat und Zeit'] },
        }],
        ['vermittlung', 'Bester Treffer: Linsen-Bolognese – vegetarisch, ohne Pilze, 25 Minuten.', {
          stack: 'Semantic Similarity (fastembed, ONNX) + feste Regeln: Allergien sind harte Filter',
          server: 'Heimserver · Berechnung lokal',
          owner: 'Ich',
          data: ['Bewertungen der Familie', '+ Preisvergleich (Plugin nachrüstbar)'],
          plugin: { name: 'Rangliste mit Begründung', icon: '🔗', lines: ['1. Linsen-Bolognese · 94 %', 'passt: vegetarisch, Vorrat, 25 min'] },
        }],
        ['andere', 'Lea bekommt eine Frage: „Bringst du geriebenen Käse mit?“', {
          stack: 'Messenger-Adapter (Matrix), ACI-Formular für die Rückfrage',
          server: 'Leas eigener Connect-Knoten · Ende-zu-Ende verschlüsselt',
          owner: 'Lea – ihr Standort und ihre Daten bleiben bei ihr',
          data: ['Nur die Frage – kein Zugriff auf Leas Daten'],
          plugin: { name: 'Ja/Nein-Karte (ACI)', icon: '🙋', lines: ['Bringst du geriebenen Käse mit?', '[ Ja ]   [ Nein ]'] },
        }],
        ['einwilligung', 'Lea tippt „Ja, bin um 18:40 da“. Geteilt wird nur diese Antwort.', {
          stack: 'Policy-Engine (deterministisch), signiertes Audit-Log',
          server: 'Beide Knoten · Eintrag signiert und nachprüfbar',
          owner: 'Lea gibt frei · Lesezugriff für mich bis 20:00',
          data: ['Einwilligung mit Ablaufzeit'],
          plugin: { name: 'Nachweis-Eintrag', icon: '✅', lines: ['Lea → Ich: „Ja, 18:40“', 'gültig bis 20:00 · signiert'] },
        }],
        ['wissen', 'Gekocht und gespeichert: Nächste Woche schlägt Connect es wieder vor – oder bewusst etwas anderes.', {
          stack: 'Wissensgraph-Eintrag, Export als JSON oder Markdown',
          server: 'Heimserver · Kopie in Leas geteiltem Bereich',
          owner: 'Familie (geteilt) · jederzeit löschbar',
          data: ['Mahlzeiten-Verlauf +1', '+ Einkaufsliste für morgen'],
          plugin: { name: 'Kochmodus auf dem Tablet', icon: '👩‍🍳', lines: ['Schritt 1/6: Zwiebeln anschwitzen', '⏱ 25 min · 3 Portionen'] },
        }],
      ],
    },
  ],
};
