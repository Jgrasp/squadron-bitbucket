use serde_json::Value;
use squadron_extension_api::forge::{Pull, Reviewer};

pub const FIELDS: &str = "values.id,values.title,values.description,values.state,values.draft,values.comment_count,values.links.html.href,values.source.branch.name,values.source.commit.hash,values.destination.branch.name,values.author.display_name,values.author.links.avatar.href,values.updated_on,values.participants.approved,values.participants.state,values.participants.role,values.participants.participated_on,values.participants.user.uuid,values.participants.user.display_name,values.participants.user.links.avatar.href,values.reviewers.uuid,values.reviewers.display_name,values.reviewers.links.avatar.href";

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
            "destination": { "branch": { "name": "feature/FIL-671" } },
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
}
