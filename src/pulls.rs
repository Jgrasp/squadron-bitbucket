use serde_json::Value;
use squadron_extension_api::forge::{Pull, Reviewer};

pub const FIELDS: &str = "values.id,values.title,values.description,values.state,values.draft,values.comment_count,values.links.html.href,values.source.branch.name,values.source.commit.hash,values.destination.branch.name,values.destination.branch.merge_strategies,values.close_source_branch,values.author.display_name,values.author.links.avatar.href,values.updated_on,values.participants.approved,values.participants.state,values.participants.role,values.participants.participated_on,values.participants.user.uuid,values.participants.user.display_name,values.participants.user.links.avatar.href,values.reviewers.uuid,values.reviewers.display_name,values.reviewers.links.avatar.href";

pub fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn verdict(person: &Value) -> &'static str {
    match (person["approved"] == true, person["state"].as_str()) {
        (true, _) => "approved",
        (_, Some("changes_requested")) => "changes_requested",
        _ if person["participated_on"].is_string() => "commented",
        _ => "waiting",
    }
}

fn checks(statuses: &Value) -> Option<String> {
    let states: Vec<&str> = list(&statuses["values"])
        .iter()
        .map(|s| match s["state"].as_str() {
            Some("SUCCESSFUL") => "pass",
            Some("INPROGRESS") => "pending",
            _ => "fail",
        })
        .collect();
    if states.is_empty() {
        None
    } else if states.contains(&"fail") {
        Some("failing".into())
    } else if states.contains(&"pending") {
        Some("pending".into())
    } else {
        Some("passing".into())
    }
}

const METHODS: [(&str, &[&str]); 3] = [
    ("merge", &["merge_commit"]),
    ("squash", &["squash", "squash_fast_forward"]),
    ("rebase", &["rebase_fast_forward"]),
];

pub fn methods(strategies: &Value) -> Vec<String> {
    let allowed = list(strategies);
    METHODS
        .iter()
        .filter(|(_, s)| s.iter().any(|s| allowed.iter().any(|a| a == s)))
        .map(|(m, _)| m.to_string())
        .collect()
}

pub fn strategy(method: &str, strategies: &Value) -> Result<&'static str, String> {
    let (_, candidates) = METHODS
        .iter()
        .find(|(m, _)| *m == method)
        .ok_or_else(|| format!("Bitbucket cannot merge with {method}."))?;
    let allowed = list(strategies);
    Ok(candidates
        .iter()
        .find(|s| allowed.iter().any(|a| a == *s))
        .unwrap_or(&candidates[0]))
}

pub fn pull_of(found: &Value, statuses: &Value, me: &str) -> Pull {
    let people = list(&found["participants"]);
    let asked = list(&found["reviewers"]);
    let mut reviewers: Vec<Reviewer> = asked
        .iter()
        .map(|user| Reviewer {
            name: text(&user["display_name"]),
            avatar: text(&user["links"]["avatar"]["href"]),
            verdict: people
                .iter()
                .find(|p| p["user"]["uuid"] == user["uuid"])
                .map_or("waiting", verdict)
                .into(),
        })
        .collect();
    for p in people {
        let listed = asked.iter().any(|u| u["uuid"] == p["user"]["uuid"]);
        if !listed && (p["role"] == "REVIEWER" || verdict(p) != "waiting") {
            reviewers.push(Reviewer {
                name: text(&p["user"]["display_name"]),
                avatar: text(&p["user"]["links"]["avatar"]["href"]),
                verdict: verdict(p).into(),
            });
        }
    }
    let checks = checks(statuses);
    let approvals = people.iter().filter(|p| p["approved"] == true).count() as u32;
    let changes_requested = people.iter().any(|p| p["state"] == "changes_requested");
    let draft = found["draft"] == true;
    let mine = people
        .iter()
        .find(|p| !me.is_empty() && p["user"]["uuid"] == me);
    Pull {
        number: found["id"].as_u64().unwrap_or_default(),
        title: text(&found["title"]),
        description: text(&found["description"]),
        url: text(&found["links"]["html"]["href"]),
        state: text(&found["state"]).to_lowercase(),
        draft,
        approvals,
        changes_requested,
        comments: found["comment_count"].as_u64().unwrap_or_default() as u32,
        mergeable: found["state"] == "OPEN"
            && !draft
            && !changes_requested
            && approvals > 0
            && checks.as_deref().is_none_or(|c| c == "passing"),
        checks,
        branch: text(&found["source"]["branch"]["name"]),
        base: text(&found["destination"]["branch"]["name"]),
        head: text(&found["source"]["commit"]["hash"]),
        author: text(&found["author"]["display_name"]),
        author_avatar: text(&found["author"]["links"]["avatar"]["href"]),
        updated: text(&found["updated_on"]),
        reviewed: mine
            .map(verdict)
            .filter(|v| *v != "waiting")
            .map(str::to_string),
        reviewed_head: None,
        reviewed_at: mine
            .and_then(|p| p["participated_on"].as_str())
            .map(str::to_string),
        head_at: None,
        reviewers,
        conflicts: false,
        merge_methods: methods(&found["destination"]["branch"]["merge_strategies"]),
        delete_branch: found["close_source_branch"].as_bool(),
        delete_branch_settable: true,
        branch_protected: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sums_up_a_bitbucket_pull_request() {
        let found = json!({
            "id": 2108, "title": "feat(prevision): [FIL-671] prévision", "state": "OPEN", "draft": false, "comment_count": 3,
            "links": { "html": { "href": "https://bitbucket.org/akki-team/fileas/pull-requests/2108" } },
            "source": { "branch": { "name": "dev/FIL-671" }, "commit": { "hash": "1094c23e6813" } },
            "destination": { "branch": { "name": "feature/FIL-671", "merge_strategies": ["merge_commit", "fast_forward", "squash_fast_forward"] } },
            "close_source_branch": true,
            "author": { "display_name": "Quentin Briais" }, "updated_on": "2026-10-23T09:27:57+00:00",
            "reviewers": [{ "uuid": "{a}", "display_name": "Ann", "links": { "avatar": { "href": "https://a/ann.png" } } }, { "uuid": "{me}", "display_name": "Me", "links": { "avatar": { "href": "https://a/me.png" } } }, { "uuid": "{c}", "display_name": "Carl", "links": { "avatar": { "href": "https://a/carl.png" } } }],
            "participants": [
                { "approved": true, "state": "approved", "role": "REVIEWER", "user": { "uuid": "{a}", "display_name": "Ann" } },
                { "approved": false, "state": "changes_requested", "role": "REVIEWER", "user": { "uuid": "{me}", "display_name": "Me" } },
                { "approved": false, "state": null, "role": "PARTICIPANT", "user": { "uuid": "{b}", "display_name": "Bob" } }
            ]
        });
        let statuses = json!({ "values": [{ "state": "SUCCESSFUL" }, { "state": "INPROGRESS" }] });
        assert_eq!(
            pull_of(&found, &statuses, "{me}"),
            Pull {
                number: 2108,
                title: "feat(prevision): [FIL-671] prévision".into(),
                description: String::new(),
                url: "https://bitbucket.org/akki-team/fileas/pull-requests/2108".into(),
                state: "open".into(),
                draft: false,
                approvals: 1,
                changes_requested: true,
                comments: 3,
                checks: Some("pending".into()),
                branch: "dev/FIL-671".into(),
                base: "feature/FIL-671".into(),
                head: "1094c23e6813".into(),
                author: "Quentin Briais".into(),
                author_avatar: String::new(),
                updated: "2026-10-23T09:27:57+00:00".into(),
                reviewed: Some("changes_requested".into()),
                reviewed_head: None,
                reviewed_at: None,
                head_at: None,
                reviewers: vec![
                    Reviewer {
                        name: "Ann".into(),
                        avatar: "https://a/ann.png".into(),
                        verdict: "approved".into()
                    },
                    Reviewer {
                        name: "Me".into(),
                        avatar: "https://a/me.png".into(),
                        verdict: "changes_requested".into()
                    },
                    Reviewer {
                        name: "Carl".into(),
                        avatar: "https://a/carl.png".into(),
                        verdict: "waiting".into()
                    },
                ],
                conflicts: false,
                mergeable: false,
                merge_methods: vec!["merge".into(), "squash".into()],
                delete_branch: Some(true),
                delete_branch_settable: true,
                branch_protected: false,
            }
        );
        assert_eq!(pull_of(&found, &Value::Null, "{b}").checks, None);
        assert_eq!(pull_of(&found, &Value::Null, "{b}").reviewed, None);
        let mut talked = found.clone();
        talked["participants"][2]["participated_on"] = json!("2026-10-23T10:00:00Z");
        let bob = pull_of(&talked, &Value::Null, "{b}");
        assert_eq!(
            (bob.reviewed.as_deref(), bob.reviewed_at.as_deref()),
            (Some("commented"), Some("2026-10-23T10:00:00Z"))
        );
        assert_eq!(
            bob.reviewers
                .last()
                .map(|r| (r.name.as_str(), r.verdict.as_str())),
            Some(("Bob", "commented"))
        );
        assert_eq!(
            pull_of(&found, &Value::Null, "{a}").reviewed.as_deref(),
            Some("approved")
        );
        assert_eq!(pull_of(&found, &Value::Null, "").reviewed, None);
        let mut approved = found.clone();
        approved["participants"][1]["state"] = Value::Null;
        assert!(pull_of(&approved, &Value::Null, "").mergeable);
        assert!(!pull_of(&approved, &statuses, "").mergeable);
        let failed = json!({ "values": [{ "state": "SUCCESSFUL" }, { "state": "FAILED" }] });
        assert_eq!(
            pull_of(&approved, &failed, "").checks.as_deref(),
            Some("failing")
        );
        let passed = json!({ "values": [{ "state": "SUCCESSFUL" }] });
        assert!(pull_of(&approved, &passed, "").mergeable);
        let mut merged = approved.clone();
        merged["state"] = json!("MERGED");
        let merged = pull_of(&merged, &passed, "");
        assert_eq!((merged.state.as_str(), merged.mergeable), ("merged", false));
    }

    #[test]
    fn offers_the_merge_methods_of_the_destination_branch() {
        let every = json!([
            "merge_commit",
            "squash",
            "fast_forward",
            "squash_fast_forward",
            "rebase_fast_forward",
            "rebase_merge"
        ]);
        assert_eq!(methods(&every), ["merge", "squash", "rebase"]);
        assert_eq!(
            methods(&json!(["fast_forward", "rebase_merge"])),
            Vec::<String>::new()
        );
        assert_eq!(methods(&Value::Null), Vec::<String>::new());
        assert_eq!(strategy("rebase", &every), Ok("rebase_fast_forward"));
        assert_eq!(strategy("squash", &every), Ok("squash"));
        assert_eq!(
            strategy("squash", &json!(["squash_fast_forward"])),
            Ok("squash_fast_forward")
        );
        assert_eq!(strategy("merge", &Value::Null), Ok("merge_commit"));
        assert!(strategy("octopus", &every).is_err());
    }
}
