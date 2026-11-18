use crate::api;
use crate::reads::Repo;
use serde_json::{Map, Value, json};
use squadron_extension_api::forge::{Draft, PostArgs, Posted, ReplyArgs, ResolveArgs, UpdateArgs};

fn comment_id(to: &str) -> Result<u64, String> {
    to.parse()
        .map_err(|_| format!("{to} is not a Bitbucket comment."))
}

fn send(repo: &Repo, method: &str, rest: &str, body: Option<Value>) -> Result<Value, String> {
    api::send(&repo.account, method, &repo.path(rest), &[], body)
}

pub fn reply(repo: &Repo, args: &ReplyArgs) -> Result<(), String> {
    let body =
        json!({ "content": { "raw": args.body }, "parent": { "id": comment_id(&args.to)? } });
    send(
        repo,
        "POST",
        &format!("/pullrequests/{}/comments", args.number),
        Some(body),
    )
    .map(|_| ())
}

pub fn resolve(repo: &Repo, args: &ResolveArgs) -> Result<(), String> {
    let id = comment_id(&args.to)?;
    send(
        repo,
        if args.resolved { "POST" } else { "DELETE" },
        &format!("/pullrequests/{}/comments/{id}/resolve", args.number),
        None,
    )
    .map(|_| ())
}

pub fn comment(draft: &Draft) -> Value {
    let mut comment = json!({ "content": { "raw": draft.body } });
    if !draft.path.is_empty() {
        comment["inline"] = json!({ "path": draft.path });
        comment["inline"][if draft.removed { "from" } else { "to" }] = json!(draft.line);
    }
    comment
}

pub fn decision(verdict: &str) -> Option<&'static str> {
    match verdict {
        "approve" => Some("approve"),
        "request_changes" => Some("request-changes"),
        _ => None,
    }
}

pub fn plain(error: String) -> String {
    match serde_json::from_str::<Value>(&error) {
        Ok(found) => found["title"]["en"]
            .as_str()
            .or(found["title"].as_str())
            .map(str::to_string)
            .unwrap_or(error),
        Err(_) => error,
    }
}

pub fn post(repo: &Repo, args: &PostArgs) -> Result<Posted, String> {
    let comments = format!("/pullrequests/{}/comments", args.number);
    for (done, draft) in args.drafts.iter().enumerate() {
        if let Err(error) = send(repo, "POST", &comments, Some(comment(draft))) {
            if done == 0 && api::denied(&error) {
                return Err(error);
            }
            return Ok(Posted {
                done,
                said: false,
                error: Some(plain(error)),
            });
        }
    }
    let summary = args.summary.trim();
    let mut sent = if summary.is_empty() {
        Ok(Value::Null)
    } else {
        send(
            repo,
            "POST",
            &comments,
            Some(json!({ "content": { "raw": summary } })),
        )
    };
    if args.drafts.is_empty() {
        if let Err(error) = &sent {
            if api::denied(error) {
                return Err(error.clone());
            }
        }
    }
    let said = sent.is_ok();
    if let (true, Some(decision)) = (said, decision(&args.verdict)) {
        sent = send(
            repo,
            "POST",
            &format!("/pullrequests/{}/{decision}", args.number),
            None,
        );
    }
    Ok(Posted {
        done: args.drafts.len(),
        said,
        error: sent.err().map(plain),
    })
}

pub fn merge(repo: &Repo, number: u64) -> Result<(), String> {
    send(
        repo,
        "POST",
        &format!("/pullrequests/{number}/merge"),
        Some(json!({})),
    )
    .map(|_| ())
}

pub fn changes(args: &UpdateArgs, found: &Value) -> Value {
    let mut changes = Map::new();
    changes.insert(
        "title".into(),
        args.title
            .as_ref()
            .map_or_else(|| found["title"].clone(), |t| json!(t)),
    );
    if let Some(description) = &args.description {
        changes.insert("description".into(), json!(description));
    }
    changes.insert("reviewers".into(), found["reviewers"].clone());
    Value::Object(changes)
}

pub fn update(repo: &Repo, args: &UpdateArgs) -> Result<(), String> {
    let path = repo.path(&format!("/pullrequests/{}", args.number));
    let found = repo.get(&path, &[("fields", "title,reviewers.uuid")])?;
    api::send(
        &repo.account,
        "PUT",
        &path,
        &[],
        Some(changes(args, &found)),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(path: &str, line: u32, removed: bool, body: &str) -> Draft {
        Draft {
            path: path.into(),
            line,
            removed,
            body: body.into(),
        }
    }

    #[test]
    fn writes_line_comments_on_the_new_or_old_side() {
        assert_eq!(
            comment(&draft("src/a.rs", 4, false, "Rename")),
            json!({ "content": { "raw": "Rename" }, "inline": { "path": "src/a.rs", "to": 4 } })
        );
        assert_eq!(
            comment(&draft("src/a.rs", 9, true, "Why?")),
            json!({ "content": { "raw": "Why?" }, "inline": { "path": "src/a.rs", "from": 9 } })
        );
        assert_eq!(
            comment(&draft("", 0, false, "Add a test.")),
            json!({ "content": { "raw": "Add a test." } })
        );
    }

    #[test]
    fn gives_the_verdict_its_bitbucket_endpoint() {
        assert_eq!(decision("approve"), Some("approve"));
        assert_eq!(decision("request_changes"), Some("request-changes"));
        assert_eq!(decision("comment"), None);
        assert_eq!(decision(""), None);
    }

    #[test]
    fn keeps_the_title_and_reviewers_when_updating() {
        let found = json!({ "title": "Old", "reviewers": [{ "uuid": "{a}" }] });
        let description = UpdateArgs {
            number: 3,
            title: None,
            description: Some("Adds export".into()),
        };
        assert_eq!(
            changes(&description, &found),
            json!({ "title": "Old", "description": "Adds export", "reviewers": [{ "uuid": "{a}" }] })
        );
        let title = UpdateArgs {
            number: 3,
            title: Some("Export".into()),
            description: None,
        };
        assert_eq!(
            changes(&title, &found),
            json!({ "title": "Export", "reviewers": [{ "uuid": "{a}" }] })
        );
    }

    #[test]
    fn reads_a_comment_id() {
        assert_eq!(comment_id("42"), Ok(42));
        assert!(comment_id("x:1").is_err());
        assert_eq!(
            plain(api::refused()).split(':').next(),
            Some("Bitbucket refused the API token")
        );
        assert_eq!(
            plain("Bitbucket answered 400: no".into()),
            "Bitbucket answered 400: no"
        );
    }
}
