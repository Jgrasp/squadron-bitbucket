use crate::api;
use crate::pulls;
use crate::reads::{self, Repo};
use serde_json::{Map, Value, json};
use squadron_extension_api::forge::{
    self, CreateArgs, Draft, MergeArgs, PostArgs, Posted, Pull, ReplyArgs, ResolveArgs, UpdateArgs,
};

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

pub fn merging(args: &MergeArgs, found: &Value) -> Result<Value, String> {
    let mut body = json!({});
    let branch = &found["destination"]["branch"];
    match &args.method {
        Some(method) => {
            body["merge_strategy"] = json!(pulls::strategy(method, &branch["merge_strategies"])?)
        }
        None if branch["default_merge_strategy"].is_string() => {
            body["merge_strategy"] = branch["default_merge_strategy"].clone()
        }
        None => {}
    }
    if let Some(close) = args
        .delete_branch
        .or(found["close_source_branch"].as_bool())
    {
        body["close_source_branch"] = json!(close);
    }
    Ok(body)
}

pub fn merge(repo: &Repo, args: &MergeArgs) -> Result<(), String> {
    let path = format!("/pullrequests/{}", args.number);
    let found = repo.get(
        &repo.path(&path),
        &[(
            "fields",
            "close_source_branch,destination.branch.merge_strategies,destination.branch.default_merge_strategy",
        )],
    )?;
    send(
        repo,
        "POST",
        &format!("{path}/merge"),
        Some(merging(args, &found)?),
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

pub fn opening(args: &CreateArgs, defaults: &[Value], me: &str) -> Value {
    let mut reviewers: Vec<Value> = Vec::new();
    for uuid in defaults.iter().map(|d| &d["user"]["uuid"]) {
        let named = json!({ "uuid": uuid });
        if !me.is_empty() && uuid.is_string() && uuid != me && !reviewers.contains(&named) {
            reviewers.push(named);
        }
    }
    let mut body = json!({
        "title": args.title,
        "description": args.description,
        "source": { "branch": { "name": args.branch } },
        "draft": args.draft,
        "reviewers": reviewers,
    });
    if !args.base.is_empty() {
        body["destination"] = json!({ "branch": { "name": args.base } });
    }
    body
}

pub fn refusal(error: String, args: &CreateArgs) -> String {
    let said = error.to_lowercase();
    if !said.starts_with("bitbucket answered 400") {
        return error;
    }
    let (branch, base) = (&args.branch, &args.base);
    let (en_base, fr_base) = if base.is_empty() {
        (
            "the main branch".to_string(),
            "la branche principale".to_string(),
        )
    } else {
        (base.clone(), base.clone())
    };
    let title = if said.contains("no changes") {
        json!({
            "en": format!("{branch} has no changes to merge into {en_base}"),
            "fr": format!("{branch} n’a aucun changement à fusionner dans {fr_base}"),
        })
    } else if said.contains("already") {
        json!({
            "en": format!("A pull request from {branch} into {en_base} is already open"),
            "fr": format!("Une pull request de {branch} vers {fr_base} est déjà ouverte"),
        })
    } else {
        return error;
    };
    forge::failure(None, title, None)
}

pub fn create(repo: &Repo, args: &CreateArgs) -> Result<Pull, String> {
    let me = reads::uuid(repo).unwrap_or_default();
    let defaults = if me.is_empty() {
        Vec::new()
    } else {
        api::every(
            &repo.account,
            &repo.path("/effective-default-reviewers"),
            &[("fields", "next,values.user.uuid"), ("pagelen", "100")],
        )
        .unwrap_or_default()
    };
    let made = send(
        repo,
        "POST",
        "/pullrequests",
        Some(opening(args, &defaults, &me)),
    )
    .map_err(|e| refusal(e, args))?;
    let number = made["id"].as_u64().unwrap_or_default();
    Ok(reads::by_number(repo, number).unwrap_or_else(|_| pulls::pull_of(&made, &Value::Null, &me)))
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
    fn always_says_whether_merging_closes_the_source_branch() {
        let found = json!({ "close_source_branch": true, "destination": { "branch": { "merge_strategies": ["merge_commit", "squash_fast_forward"] } } });
        let args = |method: Option<&str>, delete_branch| MergeArgs {
            number: 7,
            method: method.map(str::to_string),
            delete_branch,
        };
        assert_eq!(
            merging(&args(None, None), &found),
            Ok(json!({ "close_source_branch": true }))
        );
        assert_eq!(
            merging(&args(Some("squash"), Some(false)), &found),
            Ok(json!({ "merge_strategy": "squash_fast_forward", "close_source_branch": false }))
        );
        assert_eq!(merging(&args(None, None), &json!({})), Ok(json!({})));
        let squashing =
            json!({ "destination": { "branch": { "default_merge_strategy": "squash" } } });
        assert_eq!(
            merging(&args(None, Some(true)), &squashing),
            Ok(json!({ "merge_strategy": "squash", "close_source_branch": true }))
        );
    }

    fn creating(base: &str) -> CreateArgs {
        CreateArgs {
            branch: "feat/x".into(),
            base: base.into(),
            title: "Export".into(),
            description: "Sends the **ledger**".into(),
            draft: true,
        }
    }

    #[test]
    fn opens_a_pull_request_with_the_default_reviewers_but_the_author() {
        let defaults = [
            json!({ "user": { "uuid": "{a}" } }),
            json!({ "user": { "uuid": "{me}" } }),
            json!({ "user": { "uuid": "{a}" } }),
            json!({ "reviewer_type": "project" }),
        ];
        assert_eq!(
            opening(&creating("main"), &defaults, "{me}"),
            json!({
                "title": "Export",
                "description": "Sends the **ledger**",
                "source": { "branch": { "name": "feat/x" } },
                "destination": { "branch": { "name": "main" } },
                "draft": true,
                "reviewers": [{ "uuid": "{a}" }],
            })
        );
        let unknown = opening(&creating(""), &defaults, "");
        assert_eq!(unknown["reviewers"], json!([]));
        assert!(unknown.get("destination").is_none());
    }

    #[test]
    fn explains_why_bitbucket_did_not_open_the_pull_request() {
        let title = |error: &str, base: &str| {
            let found: Value = serde_json::from_str(&refusal(error.into(), &creating(base)))
                .unwrap_or(json!({ "title": { "en": error } }));
            (found["title"]["en"].clone(), found["title"]["fr"].clone())
        };
        assert_eq!(
            title(
                "Bitbucket answered 400: There are no changes to be pulled",
                "main"
            ),
            (
                json!("feat/x has no changes to merge into main"),
                json!("feat/x n’a aucun changement à fusionner dans main")
            )
        );
        assert_eq!(
            title("Bitbucket answered 400: A pull request already exists", "").0,
            json!("A pull request from feat/x into the main branch is already open")
        );
        assert_eq!(
            refusal(
                "Bitbucket answered 400: bad title".into(),
                &creating("main")
            ),
            "Bitbucket answered 400: bad title"
        );
        assert_eq!(
            refusal(
                "Bitbucket answered 404: no changes".into(),
                &creating("main")
            ),
            "Bitbucket answered 404: no changes"
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
