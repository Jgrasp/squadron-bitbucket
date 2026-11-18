use serde_json::{Value, json};
use squadron_extension_api::forge;
use squadron_extension_api::{Request, Response, host};

pub const API: &str = "https://api.bitbucket.org/2.0";

pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn url(path: &str, query: &[(&str, &str)]) -> String {
    let base = if path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{API}{path}")
    };
    if query.is_empty() {
        return base;
    }
    let query: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect();
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}{}", query.join("&"))
}

pub fn request(method: &str, url: String, account: &str, body: Option<String>) -> Request {
    let mut headers = vec![
        (
            "Authorization".into(),
            format!("Basic {{{{base64:{account}.email:{account}.token}}}}"),
        ),
        ("Accept".into(), "application/json".into()),
    ];
    if body.is_some() {
        headers.push(("Content-Type".into(), "application/json".into()));
    }
    Request {
        method: method.into(),
        url,
        headers,
        body,
    }
}

pub fn get(account: &str, path: &str, query: &[(&str, &str)]) -> Result<Response, String> {
    host::fetch(&request("GET", url(path, query), account, None))
}

pub fn refused() -> String {
    forge::no_account(json!({
        "en": "Bitbucket refused the API token: add a new one in Settings › Extensions › Bitbucket.",
        "fr": "Bitbucket a refusé le jeton d’API : ajoutez-en un nouveau dans Réglages › Extensions › Bitbucket.",
    }))
}

pub fn failure(status: u16, body: &Value, raw: &str) -> String {
    let reason = [&body["error"]["message"], &body["message"], &body["error"]]
        .into_iter()
        .find_map(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| raw.chars().take(200).collect());
    format!("Bitbucket answered {status}: {reason}")
}

pub fn answer(response: Response) -> Result<Value, String> {
    if response.status == 401 {
        return Err(refused());
    }
    let body: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
    if !(200..300).contains(&response.status) {
        return Err(failure(response.status, &body, &response.body));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_queries_bitbucket_reads() {
        assert_eq!(
            url(
                "/repositories/a/b/pullrequests",
                &[
                    ("q", "source.branch.name=\"feat/x y\""),
                    ("state", "OPEN"),
                    ("state", "MERGED"),
                    ("fields", "values.id,values.title"),
                ]
            ),
            "https://api.bitbucket.org/2.0/repositories/a/b/pullrequests?q=source.branch.name%3D%22feat%2Fx%20y%22&state=OPEN&state=MERGED&fields=values.id%2Cvalues.title"
        );
        assert_eq!(
            url("/user", &[("fields", "uuid")]),
            "https://api.bitbucket.org/2.0/user?fields=uuid"
        );
        assert_eq!(encode("{ab-12}é"), "%7Bab-12%7D%C3%A9");
        let next = "https://api.bitbucket.org/2.0/repositories/a/b/pullrequests/3/comments?page=2";
        assert_eq!(url(next, &[]), next);
    }

    #[test]
    fn explains_why_bitbucket_refused() {
        let body = json!({ "type": "error", "error": { "message": "Your credentials lack one or more required privilege scopes." } });
        assert_eq!(
            failure(403, &body, ""),
            "Bitbucket answered 403: Your credentials lack one or more required privilege scopes."
        );
        assert_eq!(
            failure(502, &Value::Null, "Bad gateway"),
            "Bitbucket answered 502: Bad gateway"
        );
        let refused: Value = serde_json::from_str(&refused()).unwrap();
        assert_eq!(refused["code"], "account");
    }

    #[test]
    fn asks_squadron_for_basic_auth_with_the_account() {
        let sent = request("POST", url("/user", &[]), "acc1", Some("{}".into()));
        assert_eq!(
            sent.headers[0],
            (
                "Authorization".to_string(),
                "Basic {{base64:acc1.email:acc1.token}}".to_string()
            )
        );
        assert!(
            sent.headers
                .iter()
                .any(|(k, v)| k == "Content-Type" && v == "application/json")
        );
    }
}
