//! Telegram Desktop export, machine-readable JSON (`result.json`).
//!
//! Either one chat (`{"name", "type", "id", "messages": [...]}`) or the whole account
//! (`{"chats": {"list": [chat, ...]}}`). Only messages; service entries (joined, pinned, …)
//! are left out.

use serde_json::Value;
use symbiot_core::evidence::{Actor, Evidence, role};

use crate::{Error, Result};

pub fn parse(json: &str) -> Result<Vec<Evidence>> {
    let v: Value = serde_json::from_str(json)?;
    let chats: Vec<&Value> = if let Some(list) = v.pointer("/chats/list").and_then(Value::as_array)
    {
        list.iter().collect()
    } else if v.get("messages").is_some() {
        vec![&v]
    } else {
        return Err(Error::Format(
            "kein Telegram-Export: weder „messages“ noch „chats.list“ gefunden".into(),
        ));
    };
    let mut out = Vec::new();
    for chat in chats {
        let chat_id = id_text(&chat["id"]).unwrap_or_else(|| "unbekannt".into());
        let chat_name = chat["name"].as_str().unwrap_or("");
        let chat_type = chat["type"].as_str().unwrap_or("");
        for m in chat["messages"].as_array().into_iter().flatten() {
            if m["type"].as_str() != Some("message") {
                continue;
            }
            let Some(msg_id) = id_text(&m["id"]) else {
                continue;
            };
            let Some(at_ms) = m["date_unixtime"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .map(|s| s * 1000)
                .filter(|&t| t > 0)
            else {
                continue;
            };
            let mut actors = Vec::new();
            if let Some(from_id) = m["from_id"].as_str().filter(|s| !s.is_empty()) {
                actors.push(Actor {
                    id: format!("telegram:{from_id}"),
                    role: role::FROM.into(),
                    name: m["from"].as_str().unwrap_or("").into(),
                });
            }
            let mut fields = vec![
                ("chat".to_string(), chat_name.to_string()),
                ("chat_type".to_string(), chat_type.to_string()),
            ];
            if let Some(reply) = id_text(&m["reply_to_message_id"]) {
                fields.push(("reply_to".into(), format!("telegram:{chat_id}/{reply}")));
            }
            for key in ["photo", "file"] {
                if let Some(path) = m[key].as_str() {
                    fields.push(("attachment".into(), path.into()));
                }
            }
            if let Some(edited) = m["edited_unixtime"].as_str() {
                fields.push(("edited_unixtime".into(), edited.into()));
            }
            out.push(Evidence {
                id: format!("telegram:{chat_id}/{msg_id}"),
                kind: "schema:Message".into(),
                at_ms,
                actors,
                text: text_of(&m["text"]),
                fields,
                files: vec![],
            });
        }
    }
    Ok(out)
}

/// Ids are numbers in some exports and strings in others.
fn id_text(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// `text` is a string, or a list of strings and entities (`{"type": "bold", "text": "…"}`).
fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p {
                Value::String(s) => s.as_str(),
                other => other["text"].as_str().unwrap_or(""),
            })
            .collect(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAT: &str = r#"{
      "name": "Familie", "type": "private_group", "id": 4242,
      "messages": [
        {"id": 1, "type": "service", "date_unixtime": "1700000000", "action": "create_group"},
        {"id": 2, "type": "message", "date_unixtime": "1700000060", "from": "Anna",
         "from_id": "user111", "text": "Hallo zusammen"},
        {"id": 3, "type": "message", "date_unixtime": "1700000120", "from": "Ben",
         "from_id": "user222", "reply_to_message_id": 2,
         "text": ["Schau mal ", {"type": "link", "text": "https://example.org"}],
         "photo": "photos/photo_1.jpg"}
      ]
    }"#;

    #[test]
    fn one_chat() {
        let items = parse(CHAT).unwrap();
        assert_eq!(items.len(), 2, "service entries are left out");
        let a = &items[0];
        assert_eq!(a.id, "telegram:4242/2");
        assert_eq!(a.at_ms, 1_700_000_060_000);
        assert_eq!(a.text, "Hallo zusammen");
        assert_eq!(a.actors[0].id, "telegram:user111");
        assert_eq!(a.actors[0].name, "Anna");
        let b = &items[1];
        assert_eq!(b.text, "Schau mal https://example.org");
        assert!(
            b.fields
                .contains(&("reply_to".into(), "telegram:4242/2".into()))
        );
        assert!(
            b.fields
                .contains(&("attachment".into(), "photos/photo_1.jpg".into()))
        );
        assert!(items.iter().all(|e| e.check().is_ok()));
    }

    #[test]
    fn whole_account() {
        let account = format!(r#"{{"chats": {{"list": [{CHAT}, {CHAT}]}}}}"#);
        assert_eq!(parse(&account).unwrap().len(), 4);
    }

    #[test]
    fn not_an_export() {
        assert!(matches!(parse(r#"{"foo": 1}"#), Err(Error::Format(_))));
        assert!(matches!(parse("nein"), Err(Error::Json(_))));
    }
}
