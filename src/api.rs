use serde_json::{Value, json};
use squadron_extension_api::forge;
use squadron_extension_api::{Request, Response, host};

pub const API: &str = "https://api.bitbucket.org/2.0";

pub fn no_user_scope() -> String {
    forge::failure(
        None,
        json!({
            "en": "Add the read:user:bitbucket scope to the Bitbucket API token to list your pull requests.",
            "fr": "Ajoutez le scope read:user:bitbucket au jeton d’API Bitbucket pour lister vos pull requests.",
        }),
        None,
    )
}

pub fn no_pipeline_scope() -> String {
    forge::failure(
        None,
        json!({
            "en": "Add the read:pipeline:bitbucket scope to the Bitbucket API token to see pipelines.",
            "fr": "Ajoutez le scope read:pipeline:bitbucket au jeton d’API Bitbucket pour voir les pipelines.",
        }),
        None,
    )
}
const MOST: usize = 1000;

pub fn accounts() -> Result<Vec<String>, String> {
    let found: Vec<String> = host::accounts().into_iter().map(|a| a.id).collect();
    if found.is_empty() {
        return Err(forge::no_account(json!({
            "en": "Add a Bitbucket account in Settings › Extensions › Bitbucket.",
            "fr": "Ajoutez un compte Bitbucket dans Réglages › Extensions › Bitbucket.",
        })));
    }
    Ok(found)
}

pub fn email(account: &str) -> String {
    host::accounts()
        .into_iter()
        .find(|a| a.id == account)
        .and_then(|a| a.fields.into_iter().find(|(k, _)| k == "email"))
        .map(|(_, v)| v.to_lowercase())
        .unwrap_or_default()
}

pub fn ordered(mut accounts: Vec<String>, known: Option<String>) -> Vec<String> {
    if let Some(i) = known.and_then(|k| accounts.iter().position(|a| *a == k)) {
        let first = accounts.remove(i);
        accounts.insert(0, first);
    }
    accounts
}

pub fn scoped(error: &str) -> bool {
    error == no_user_scope() || error == no_pipeline_scope()
}

pub fn denied(error: &str) -> bool {
    scoped(error)
        || ["\"code\":\"account\"", "answered 403", "answered 404"]
            .iter()
            .any(|mark| error.contains(mark))
}

pub fn send(
    account: &str,
    method: &str,
    path: &str,
    query: &[(&str, &str)],
    body: Option<Value>,
) -> Result<Value, String> {
    let request = request(
        method,
        url(path, query),
        account,
        body.map(|b| b.to_string()),
    );
    answer(host::fetch(&request)?)
}

pub fn every(account: &str, path: &str, query: &[(&str, &str)]) -> Result<Vec<Value>, String> {
    let mut found = Vec::new();
    let mut next = url(path, query);
    loop {
        let page = send(account, "GET", &next, &[], None)?;
        found.extend(page["values"].as_array().cloned().unwrap_or_default());
        match page["next"].as_str() {
            Some(link) if link.starts_with(API) && found.len() < MOST => next = link.into(),
            _ => return Ok(found),
        }
    }
}

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
    fn tries_the_account_that_reached_the_workspace_first() {
        let all = vec!["a1".to_string(), "a2".into(), "a3".into()];
        assert_eq!(ordered(all.clone(), Some("a2".into())), ["a2", "a1", "a3"]);
        assert_eq!(ordered(all.clone(), Some("gone".into())), all);
        assert_eq!(ordered(all.clone(), None), all);
    }

    #[test]
    fn tells_a_refused_repository_from_another_failure() {
        assert!(denied(&refused()));
        assert!(denied("Bitbucket answered 404: Repository not found"));
        assert!(denied("Bitbucket answered 403: Forbidden"));
        assert!(denied(&no_pipeline_scope()) && scoped(&no_user_scope()));
        assert!(!scoped("Bitbucket answered 403: Forbidden"));
        assert!(!denied("Bitbucket answered 400: Bad request"));
    }

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
