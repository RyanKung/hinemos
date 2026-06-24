use std::collections::HashMap;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedEmail {
    pub(super) subject: String,
    pub(super) body: String,
}

pub(super) fn parse_email_message(raw: &str) -> ParsedEmail {
    let normalized = raw.replace("\r\n", "\n");
    let (headers, body) = normalized.split_once("\n\n").unwrap_or(("", &normalized));
    let headers = parse_headers(headers);
    let body = decode_message_body(body.trim_end_matches('\n'), &headers);
    ParsedEmail {
        subject: headers
            .get("subject")
            .cloned()
            .unwrap_or_else(|| "Private mail".to_owned()),
        body,
    }
}

fn parse_headers(input: &str) -> HashMap<String, String> {
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut current_key = String::new();
    for line in input.lines() {
        if line.starts_with([' ', '\t']) && !current_key.is_empty() {
            if let Some(value) = headers.get_mut(&current_key) {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        current_key = key.trim().to_ascii_lowercase();
        headers.insert(current_key.clone(), value.trim().to_owned());
    }
    headers
}

fn decode_message_body(body: &str, headers: &HashMap<String, String>) -> String {
    let Some(encoding) = headers.get("content-transfer-encoding") else {
        return body.to_owned();
    };
    if !encoding.trim().eq_ignore_ascii_case("base64") {
        return body.to_owned();
    }
    let compact: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let Some(decoded) = decode_base64_text(&compact) else {
        return body.to_owned();
    };
    decoded.trim_end_matches(['\r', '\n']).to_owned()
}

pub(super) async fn read_smtp_data(reader: &mut BufReader<TcpStream>) -> Result<String> {
    let mut data = String::new();
    loop {
        let Some(line) = read_protocol_line(reader).await? else {
            break;
        };
        if line == "." {
            break;
        }
        let line = line.strip_prefix("..").unwrap_or(&line);
        data.push_str(line);
        data.push_str("\r\n");
    }
    Ok(data)
}

pub(super) async fn read_protocol_line(
    reader: &mut BufReader<TcpStream>,
) -> Result<Option<String>> {
    let mut line = String::new();
    let read = reader.read_line(&mut line).await?;
    if read == 0 {
        return Ok(None);
    }
    Ok(Some(
        line.trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_owned(),
    ))
}

pub(super) async fn write_line(reader: &mut BufReader<TcpStream>, line: &str) -> Result<()> {
    let stream = reader.get_mut();
    stream.write_all(line.as_bytes()).await?;
    stream.write_all(b"\r\n").await?;
    stream.flush().await?;
    Ok(())
}

pub(super) async fn tagged_ok(
    reader: &mut BufReader<TcpStream>,
    tag: &str,
    message: &str,
) -> Result<()> {
    write_line(reader, &format!("{tag} OK {message}")).await
}

pub(super) async fn tagged_no(
    reader: &mut BufReader<TcpStream>,
    tag: &str,
    message: &str,
) -> Result<()> {
    write_line(reader, &format!("{tag} NO {message}")).await
}

pub(super) async fn tagged_bad(
    reader: &mut BufReader<TcpStream>,
    tag: &str,
    message: &str,
) -> Result<()> {
    write_line(reader, &format!("{tag} BAD {message}")).await
}

pub(super) fn split_command(input: &str) -> (String, &str) {
    let (command, rest) = split_first_token(input);
    (command.to_ascii_uppercase(), rest)
}

pub(super) fn split_first_token(input: &str) -> (&str, &str) {
    let input = input.trim_start();
    input
        .split_once(char::is_whitespace)
        .map_or((input, ""), |(head, rest)| (head, rest.trim_start()))
}

pub(super) fn smtp_path_after(rest: &str, marker: &str) -> Option<String> {
    let rest = rest.trim_start();
    if !rest
        .get(..marker.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(marker))
    {
        return None;
    }
    let value = rest[marker.len()..].trim();
    Some(
        value
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim()
            .to_owned(),
    )
}

pub(super) fn decode_auth_plain(input: &str) -> Option<(String, String)> {
    let decoded = decode_base64_bytes(input)?;
    let mut parts = decoded.split(|byte| *byte == 0);
    let _authorization_identity = parts.next()?;
    let username = String::from_utf8(parts.next()?.to_vec()).ok()?;
    let password = String::from_utf8(parts.next()?.to_vec()).ok()?;
    Some((username, password))
}

pub(super) fn decode_base64_text(input: &str) -> Option<String> {
    String::from_utf8(decode_base64_bytes(input)?).ok()
}

fn decode_base64_bytes(input: &str) -> Option<Vec<u8>> {
    BASE64.decode(input.trim()).ok()
}

pub(super) fn parse_imap_login(input: &str) -> Option<(String, String)> {
    let (username, rest) = parse_imap_atom_or_string(input)?;
    let (password, _) = parse_imap_atom_or_string(rest)?;
    Some((username, password))
}

fn parse_imap_atom_or_string(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix('"') {
        let mut escaped = false;
        let mut value = String::new();
        for (index, character) in rest.char_indices() {
            if escaped {
                value.push(character);
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '"' => return Some((value, &rest[index + 1..])),
                _ => value.push(character),
            }
        }
        None
    } else {
        let (value, rest) = split_first_token(input);
        (!value.is_empty()).then(|| (value.to_owned(), rest))
    }
}

pub(super) fn sanitize_header(input: &str) -> String {
    input.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::{decode_auth_plain, parse_email_message, parse_imap_login, smtp_path_after};
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;

    #[test]
    fn decodes_auth_plain() {
        let encoded = BASE64.encode(b"\0alice\0secret");

        assert_eq!(
            decode_auth_plain(&encoded),
            Some(("alice".to_owned(), "secret".to_owned()))
        );
    }

    #[test]
    fn parses_quoted_imap_login() {
        assert_eq!(
            parse_imap_login("\"alice\" \"s e c r e t\""),
            Some(("alice".to_owned(), "s e c r e t".to_owned()))
        );
    }

    #[test]
    fn extracts_smtp_paths_case_insensitively() {
        assert_eq!(
            smtp_path_after("to:<bob@hinemos.local>", "TO:"),
            Some("bob@hinemos.local".to_owned())
        );
    }

    #[test]
    fn parses_email_subject_and_body() {
        let parsed = parse_email_message("Subject: Hello\r\nFrom: alice\r\n\r\nBody\r\n");

        assert_eq!(parsed.subject, "Hello");
        assert_eq!(parsed.body, "Body");
    }

    #[test]
    fn decodes_base64_transfer_encoded_body() {
        let parsed = parse_email_message(
            "Subject: Tarot\r\nContent-Transfer-Encoding: base64\r\n\r\nSGVsbG8sIHNlZWtlci4=\r\n",
        );

        assert_eq!(parsed.subject, "Tarot");
        assert_eq!(parsed.body, "Hello, seeker.");
    }
}
