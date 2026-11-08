//! WhatsApp „Chat exportieren“ (`.txt`).
//!
//! The format depends on the phone and its language:
//! - Android, German: `12.03.24, 14:05 - Anna: Hallo`
//! - iPhone: `[12.03.24, 14:05:33] Anna: Hallo`
//! - English: `3/12/24, 2:05 PM - Anna: Hello`
//!
//! Lines without a date continue the previous message. Lines with a date but without
//! `Name: ` are system messages and are left out. Times are local; `utc_offset_min` says
//! how far ahead of UTC they are. WhatsApp exports have no message ids, so the id is a
//! fingerprint of time, sender, text and how often that combination came before.

use std::collections::HashMap;

use symbiot_core::evidence::{Actor, Evidence, role};

use crate::{Error, Result, civil_ms, fingerprint};

pub fn parse(text: &str, chat: &str, utc_offset_min: i32) -> Result<Vec<Evidence>> {
    let mut out: Vec<Evidence> = Vec::new();
    let mut in_system = false;
    let mut seen_header = false;
    for raw in text.lines() {
        let line = clean(raw);
        match header(&line) {
            Some((local_ms, rest)) => {
                seen_header = true;
                let Some((name, body)) = rest.split_once(": ") else {
                    in_system = true;
                    continue;
                };
                in_system = false;
                let at_ms = shift(local_ms, utc_offset_min);
                out.push(Evidence {
                    id: String::new(),
                    kind: "schema:Message".into(),
                    at_ms,
                    actors: vec![actor(name.trim())],
                    text: body.to_string(),
                    fields: vec![("chat".into(), chat.into())],
                    files: vec![],
                });
            }
            None => {
                if let (false, Some(last)) = (in_system, out.last_mut()) {
                    last.text.push('\n');
                    last.text.push_str(&line);
                }
            }
        }
    }
    if !seen_header && !text.trim().is_empty() {
        return Err(Error::Format(
            "kein WhatsApp-Export: keine Zeile beginnt mit Datum und Uhrzeit".into(),
        ));
    }
    let mut count: HashMap<String, u32> = HashMap::new();
    for e in &mut out {
        let base = fingerprint(&[
            e.at_ms.to_string().as_bytes(),
            e.actors[0].id.as_bytes(),
            e.text.as_bytes(),
        ]);
        let n = count.entry(base.clone()).or_insert(0);
        *n += 1;
        e.id = format!("whatsapp:{}/{base}-{n}", chat_key(chat));
    }
    Ok(out)
}

/// Removes the invisible marks iPhones add and turns special spaces into normal ones.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{feff}'))
        .map(|c| {
            if matches!(c, '\u{202f}' | '\u{a0}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// A chat name usable in an id: no slashes, no whitespace at the ends.
fn chat_key(chat: &str) -> String {
    chat.trim().replace('/', "_")
}

/// A phone number becomes `tel:`, anything else `name:`.
fn actor(name: &str) -> Actor {
    let digits: String = name.chars().filter(|c| c.is_ascii_digit()).collect();
    let phone = name.starts_with('+')
        && digits.len() >= 7
        && name
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '-' | '(' | ')'));
    Actor {
        id: if phone {
            format!("tel:+{digits}")
        } else {
            format!("name:{name}")
        },
        role: role::FROM.into(),
        name: if phone { String::new() } else { name.into() },
    }
}

fn shift(local_ms: u64, utc_offset_min: i32) -> u64 {
    let offset = utc_offset_min as i64 * 60_000;
    (local_ms as i64 - offset).max(1) as u64
}

/// `(local time in ms, rest of the line)` if the line starts a message.
fn header(line: &str) -> Option<(u64, &str)> {
    let (stamp, rest) = if let Some(inner) = line.strip_prefix('[') {
        let (stamp, rest) = inner.split_once(']')?;
        (stamp, rest.trim_start())
    } else {
        line.split_once(" - ")?
    };
    let (date, time) = stamp.split_once(", ")?;
    Some((parse_stamp(date.trim(), time.trim())?, rest))
}

fn parse_stamp(date: &str, time: &str) -> Option<u64> {
    let upper = time.to_ascii_uppercase();
    let pm = upper.ends_with("PM");
    let twelve = pm || upper.ends_with("AM");
    let clock = if twelve {
        time[..time.len() - 2].trim()
    } else {
        time
    };
    let mut t = clock.split(':').map(|x| x.parse::<u32>().ok());
    let mut hour = t.next()??;
    let minute = t.next()??;
    let second = t.next().flatten().unwrap_or(0);
    if twelve {
        if hour == 0 || hour > 12 {
            return None;
        }
        hour = hour % 12 + if pm { 12 } else { 0 };
    }

    let sep = if date.contains('.') { '.' } else { '/' };
    let parts: Vec<u32> = date
        .split(sep)
        .map(|x| x.trim().parse().ok())
        .collect::<Option<_>>()?;
    let [a, b, y] = parts[..] else {
        return None;
    };
    // „.“ is day.month.year; „/“ with AM/PM is the US order month/day/year.
    let (day, month) = if sep == '/' && twelve { (b, a) } else { (a, b) };
    let year = if y < 100 { 2000 + y as i64 } else { y as i64 };
    civil_ms(year, month, day, hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANDROID: &str = "\
12.03.24, 14:05 - Nachrichten und Anrufe sind Ende-zu-Ende-verschlüsselt.
12.03.24, 14:05 - Anna: Hallo
wie geht's?
12.03.24, 14:06 - +49 151 2345678: Gut!
12.03.24, 14:06 - Anna: <Medien ausgeschlossen>
12.03.24, 14:06 - Anna: <Medien ausgeschlossen>
";

    #[test]
    fn android_german() {
        let items = parse(ANDROID, "Anna", 60).unwrap();
        assert_eq!(items.len(), 4, "the system line is left out");
        assert_eq!(items[0].text, "Hallo\nwie geht's?");
        assert_eq!(items[0].actors[0].id, "name:Anna");
        // 14:05 CET is 13:05 UTC.
        assert_eq!(items[0].at_ms, civil_ms(2024, 3, 12, 13, 5, 0).unwrap());
        assert_eq!(items[1].actors[0].id, "tel:+491512345678");
        assert_ne!(
            items[2].id, items[3].id,
            "same minute, same text: still two messages"
        );
        assert!(items.iter().all(|e| e.check().is_ok()));
        // A second export of the same chat gives the same ids.
        let again = parse(ANDROID, "Anna", 60).unwrap();
        assert_eq!(
            items.iter().map(|e| &e.id).collect::<Vec<_>>(),
            again.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn iphone_and_english() {
        let ios = "\u{200e}[12.03.24, 14:05:33] Anna: Hallo\n";
        let e = &parse(ios, "Anna", 0).unwrap()[0];
        assert_eq!(e.at_ms, civil_ms(2024, 3, 12, 14, 5, 33).unwrap());
        assert_eq!(e.text, "Hallo");

        let us = "3/12/24, 2:05\u{202f}PM - Anna: Hello\n";
        let e = &parse(us, "Anna", 0).unwrap()[0];
        assert_eq!(e.at_ms, civil_ms(2024, 3, 12, 14, 5, 0).unwrap());
    }

    #[test]
    fn not_an_export() {
        assert!(matches!(
            parse("irgendein Text\n", "x", 0),
            Err(Error::Format(_))
        ));
        assert!(parse("", "x", 0).unwrap().is_empty());
    }
}
