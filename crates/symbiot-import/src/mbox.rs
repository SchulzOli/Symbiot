//! Mailbox files in mbox format (Google Takeout, Thunderbird, `mutt`).
//!
//! The id is the `Message-ID` header; a mail without one gets a fingerprint of its raw bytes.
//! The text is the first plain-text body (HTML-only mails are converted by the parser).

use std::io::BufRead;

use mail_parser::mailbox::mbox::MessageIterator;
use mail_parser::{AddressList, MessageParser};
use symbiot_core::evidence::{Actor, Evidence, role};

use crate::{Error, Import, Result, file_of, fingerprint};

/// Attachments go into the file store; the raw data names them with their hash.
pub fn parse(reader: impl BufRead) -> Result<Import> {
    let parser = MessageParser::default();
    let mut out = Vec::new();
    let mut pending = Vec::new();
    let mut any = false;
    for raw in MessageIterator::new(reader) {
        let raw = raw?;
        any = true;
        let Some(mail) = parser.parse(raw.contents()) else {
            continue;
        };
        let at_s = mail
            .date()
            .map(|d| d.to_timestamp())
            .filter(|&t| t > 0)
            .or_else(|| Some(raw.internal_date() as i64).filter(|&t| t > 0));
        let Some(at_s) = at_s else {
            continue;
        };
        let id = match mail.message_id().map(str::trim).filter(|m| !m.is_empty()) {
            Some(m) => format!("mail:{}", m.trim_matches(['<', '>'])),
            None => format!("mail:sha256:{}", fingerprint(&[raw.contents()])),
        };

        let mut actors = Vec::new();
        add(&mut actors, mail.from(), role::FROM);
        add(&mut actors, mail.to(), role::TO);
        add(&mut actors, mail.cc(), role::CC);

        let mut fields = Vec::new();
        if let Some(subject) = mail.subject() {
            fields.push(("subject".to_string(), subject.to_string()));
        }
        if let Some(parent) = mail.in_reply_to().and_then(|l| l.get(0)) {
            fields.push((
                "in_reply_to".into(),
                format!("mail:{}", parent.trim_matches(['<', '>'])),
            ));
        }
        let mut files = Vec::new();
        for a in mail.attachments() {
            let name = a.attachment_name().unwrap_or("anhang").to_string();
            fields.push(("attachment".into(), name.clone()));
            let mime = a
                .content_type()
                .map(|ct| match ct.subtype() {
                    Some(sub) => format!("{}/{sub}", ct.ctype()),
                    None => ct.ctype().to_string(),
                })
                .unwrap_or_else(|| crate::mime_of(&name).to_string())
                .to_ascii_lowercase();
            if let Some((file, p)) = file_of(a.decoded().into_owned(), &name, &mime) {
                files.push(file);
                pending.push(p);
            }
        }

        out.push(Evidence {
            id,
            kind: "schema:EmailMessage".into(),
            at_ms: at_s as u64 * 1000,
            actors,
            text: mail
                .body_text(0)
                .map(|t| t.into_owned())
                .unwrap_or_default(),
            fields,
            files,
        });
    }
    if !any {
        return Err(Error::Format(
            "keine Mail gefunden: eine mbox-Datei beginnt mit „From “".into(),
        ));
    }
    Ok(Import {
        items: out,
        files: pending,
    })
}

fn add(actors: &mut Vec<Actor>, list: Option<AddressList<'_>>, role: &str) {
    for mailbox in list.iter().flat_map(|l| l.mailboxes()) {
        if let Some(addr) = mailbox.address().filter(|a| a.contains('@')) {
            actors.push(Actor {
                id: format!("mailto:{}", addr.to_lowercase()),
                role: role.into(),
                name: mailbox.name().unwrap_or("").into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MBOX: &str = "From anna@example.org Tue Mar 12 14:05:00 2024
Message-ID: <abc@example.org>
Date: Tue, 12 Mar 2024 14:05:00 +0100
From: Anna Beispiel <Anna@Example.org>
To: ich@example.net, Ben <ben@example.net>
Subject: Wandern am Samstag?
Content-Type: text/plain; charset=utf-8

Hast du Lust?
>From the archive: ja.

From ich@example.net Tue Mar 12 15:00:00 2024
Date: Tue, 12 Mar 2024 15:00:00 +0100
From: ich@example.net
To: Anna Beispiel <anna@example.org>
In-Reply-To: <abc@example.org>
Subject: Re: Wandern am Samstag?

Gerne!
";

    #[test]
    fn two_mails() {
        let items = parse(MBOX.as_bytes()).unwrap().items;
        assert_eq!(items.len(), 2);
        let a = &items[0];
        assert_eq!(a.id, "mail:abc@example.org");
        assert_eq!(a.at_ms, 1_710_248_700_000);
        assert_eq!(a.actors[0].id, "mailto:anna@example.org");
        assert_eq!(a.actors[0].name, "Anna Beispiel");
        assert_eq!(a.actors[0].role, role::FROM);
        assert_eq!(a.actors.iter().filter(|x| x.role == role::TO).count(), 2);
        assert!(a.text.starts_with("Hast du Lust?"));
        assert!(
            a.fields
                .contains(&("subject".into(), "Wandern am Samstag?".into()))
        );

        let b = &items[1];
        assert!(
            b.id.starts_with("mail:sha256:"),
            "no Message-ID: fingerprint"
        );
        assert!(
            b.fields
                .contains(&("in_reply_to".into(), "mail:abc@example.org".into()))
        );
        assert!(items.iter().all(|e| e.check().is_ok()));
        assert_eq!(parse(MBOX.as_bytes()).unwrap().items[1].id, b.id, "stable");
    }

    #[test]
    fn attachment_goes_to_the_file_store() {
        let mbox = "From a@example.org Tue Mar 12 14:05:00 2024
Message-ID: <anhang@example.org>
Date: Tue, 12 Mar 2024 14:05:00 +0100
From: a@example.org
Subject: Foto
Content-Type: multipart/mixed; boundary=\"x\"

--x
Content-Type: text/plain

Hier das Foto.
--x
Content-Type: image/png; name=\"see.png\"
Content-Disposition: attachment; filename=\"see.png\"
Content-Transfer-Encoding: base64

aGFsbG8=
--x--
";
        let got = parse(mbox.as_bytes()).unwrap();
        let f = &got.items[0].files[0];
        assert_eq!(
            (f.name.as_str(), f.mime.as_str(), f.len),
            ("see.png", "image/png", 5)
        );
        assert!(got.items[0].text.starts_with("Hier das Foto."));
        let crate::FileData::Bytes(b) = &got.files[0].data else {
            panic!("bytes from inside the mail");
        };
        assert_eq!(b, b"hallo");
    }

    #[test]
    fn not_an_mbox() {
        assert!(matches!(parse("".as_bytes()), Err(Error::Format(_))));
    }
}
