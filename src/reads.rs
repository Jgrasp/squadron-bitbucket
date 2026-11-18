use crate::api;
use crate::pulls::{self, FIELDS, text};
use serde_json::Value;
use squadron_extension_api::forge::{Lists, Numbered, Person, Pull, Remark, Run};
use squadron_extension_api::host;
use std::collections::BTreeMap;

pub struct Repo {
    pub account: String,
    pub owner: String,
    pub name: String,
}

pub fn owner_and_name(
    repo: &str,
    groups: &BTreeMap<String, String>,
) -> Result<(String, String), String> {
    match (groups.get("owner"), groups.get("name")) {
        (Some(owner), Some(name)) => Ok((owner.clone(), name.clone())),
        _ => repo
            .split_once('/')
            .filter(|(o, n)| !o.is_empty() && !n.is_empty() && !n.contains('/'))
            .map(|(o, n)| (o.to_string(), n.to_string()))
            .ok_or_else(|| format!("{repo} is not a Bitbucket repository.")),
    }
}

impl Repo {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    pub fn path(&self, rest: &str) -> String {
        format!("/repositories/{}{rest}", self.slug())
    }

    pub fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        api::send(&self.account, "GET", path, query, None)
    }
}

const STATUSES: [(&str, &str); 2] = [("fields", "values.state"), ("pagelen", "50")];

fn statuses(repo: &Repo, number: &Value) -> Value {
    repo.get(
        &repo.path(&format!("/pullrequests/{number}/statuses")),
        &STATUSES,
    )
    .unwrap_or_default()
}

pub fn uuid(repo: &Repo) -> Result<String, String> {
    let key = format!("uuid:{}:{}", repo.account, api::email(&repo.account));
    if let Some(found) = host::state_get(&key) {
        return Ok(found);
    }
    let user = repo.get("/user", &[("fields", "uuid")]).map_err(|e| {
        if e.contains("answered 403") {
            api::no_user_scope()
        } else {
            e
        }
    })?;
    let uuid = user["uuid"]
        .as_str()
        .ok_or_else(api::no_user_scope)?
        .to_string();
    host::state_set(&key, Some(&uuid));
    Ok(uuid)
}

pub fn branch_query(branch: &str) -> String {
    format!(
        "source.branch.name=\"{}\"",
        branch.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

pub fn pull(repo: &Repo, branch: &str) -> Result<Option<Pull>, String> {
    let list = repo.get(
        &repo.path("/pullrequests"),
        &[
            ("q", &branch_query(branch)),
            ("state", "OPEN"),
            ("state", "MERGED"),
            ("state", "DECLINED"),
            ("sort", "-updated_on"),
            ("pagelen", "1"),
            ("fields", FIELDS),
        ],
    )?;
    let Some(found) = list["values"].get(0) else {
        return Ok(None);
    };
    let me = uuid(repo).unwrap_or_default();
    Ok(Some(pulls::pull_of(
        found,
        &statuses(repo, &found["id"]),
        &me,
    )))
}

pub fn diffstat(stat: Option<&Value>) -> (Option<u64>, Option<u64>, Option<u64>) {
    let files = stat.and_then(|s| s["values"].as_array());
    let sum = |key: &str| files.map(|f| f.iter().filter_map(|x| x[key].as_u64()).sum());
    (
        sum("lines_added"),
        sum("lines_removed"),
        files.map(|f| f.len() as u64),
    )
}

pub fn numbered(repo: &Repo, number: u64) -> Result<Numbered, String> {
    let path = repo.path(&format!("/pullrequests/{number}"));
    let found = repo.get(&path, &[("fields", &FIELDS.replace("values.", ""))])?;
    let stat = repo
        .get(
            &format!("{path}/diffstat"),
            &[
                ("fields", "values.lines_added,values.lines_removed"),
                ("pagelen", "500"),
            ],
        )
        .ok();
    let (additions, deletions, files) = diffstat(stat.as_ref());
    let me = uuid(repo).unwrap_or_default();
    Ok(Numbered {
        pull: pulls::pull_of(&found, &statuses(repo, &found["id"]), &me),
        additions,
        deletions,
        files,
    })
}

pub fn sorted(found: [Vec<Value>; 3], me: &str) -> (Vec<Pull>, Vec<Value>, Vec<Pull>) {
    let [review, mine, open] = found;
    let others = open
        .iter()
        .filter(|p| !review.iter().chain(&mine).any(|f| f["id"] == p["id"]))
        .map(|p| pulls::pull_of(p, &Value::Null, me))
        .collect();
    let review = review
        .iter()
        .map(|p| pulls::pull_of(p, &Value::Null, me))
        .collect();
    (review, mine, others)
}

pub fn lists(repo: &Repo) -> Result<Lists, String> {
    let me = uuid(repo)?;
    let path = repo.path("/pullrequests");
    let mut found: [Vec<Value>; 3] = Default::default();
    for (i, q) in [
        format!("reviewers.uuid=\"{me}\" AND state=\"OPEN\""),
        format!("author.uuid=\"{me}\" AND state=\"OPEN\""),
        "state=\"OPEN\"".to_string(),
    ]
    .iter()
    .enumerate()
    {
        let page = repo.get(
            &path,
            &[
                ("q", q),
                ("sort", "-updated_on"),
                ("pagelen", "50"),
                ("fields", FIELDS),
            ],
        )?;
        found[i] = page["values"].as_array().cloned().unwrap_or_default();
    }
    let (mut review, mine, others) = sorted(found, &me);
    for pull in review.iter_mut().filter(|p| p.reviewed_at.is_some()) {
        pull.head_at = repo
            .get(
                &repo.path(&format!("/commit/{}", pull.head)),
                &[("fields", "date")],
            )
            .ok()
            .and_then(|c| c["date"].as_str().map(str::to_string));
    }
    let mine = mine
        .iter()
        .map(|p| pulls::pull_of(p, &statuses(repo, &p["id"]), &me))
        .collect();
    Ok(Lists {
        review,
        mine,
        others,
    })
}

const REMARK_FIELDS: &str = "next,values.id,values.content.raw,values.user.display_name,values.user.links.avatar.href,values.inline,values.parent.id,values.created_on,values.deleted,values.resolution.type";

pub fn remark_of(found: &Value) -> Option<Remark> {
    if found["deleted"] == true {
        return None;
    }
    let inline = &found["inline"];
    let (to, from) = (inline["to"].as_u64(), inline["from"].as_u64());
    let current = inline["outdated"] != true;
    let root = !found["parent"]["id"].is_u64();
    Some(Remark {
        id: found["id"].as_u64()?.to_string(),
        author: text(&found["user"]["display_name"]),
        avatar: text(&found["user"]["links"]["avatar"]["href"]),
        body: text(&found["content"]["raw"]),
        path: inline["path"].as_str().map(str::to_string),
        line: to.or(from).filter(|_| current).map(|n| n as u32),
        removed: to.is_none() && from.is_some(),
        reply_to: found["parent"]["id"].as_u64().map(|n| n.to_string()),
        at: text(&found["created_on"]),
        kind: if inline["path"].is_string() {
            "line"
        } else {
            "general"
        }
        .into(),
        resolved: root.then(|| found["resolution"].is_object()),
        commit: inline["src_rev"].as_str().map(str::to_string),
    })
}

pub fn remarks(repo: &Repo, number: u64) -> Result<Vec<Remark>, String> {
    let mut found: Vec<Remark> = api::every(
        &repo.account,
        &repo.path(&format!("/pullrequests/{number}/comments")),
        &[("pagelen", "100"), ("fields", REMARK_FIELDS)],
    )?
    .iter()
    .filter_map(remark_of)
    .collect();
    found.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(found)
}

pub fn person(user: &Value) -> Person {
    Person {
        name: text(&user["display_name"]),
        avatar: text(&user["links"]["avatar"]["href"]),
    }
}

pub fn me(repo: &Repo) -> Result<Person, String> {
    repo.get("/user", &[("fields", "display_name,links.avatar.href")])
        .map(|user| person(&user))
}

const RUN_FIELDS: &str = "values.uuid,values.build_number,values.state.name,values.state.result.name,values.state.stage.name,values.target.ref_name,values.target.selector.type,values.target.selector.pattern,values.target.commit.message,values.trigger.name,values.creator.display_name,values.creator.links.avatar.href,values.created_on,values.completed_on";

pub fn run_of(value: &Value, slug: &str) -> Run {
    let state = &value["state"];
    let status = match (
        text(&state["name"]).as_str(),
        text(&state["result"]["name"]).as_str(),
    ) {
        ("COMPLETED", "SUCCESSFUL") => "success",
        ("COMPLETED", "STOPPED" | "EXPIRED") => "cancelled",
        ("COMPLETED", _) => "failure",
        ("IN_PROGRESS", _) if state["stage"]["name"] == "PAUSED" => "queued",
        ("IN_PROGRESS", _) => "running",
        _ => "queued",
    };
    let selector = &value["target"]["selector"];
    let kind = text(&selector["type"]);
    let pattern = text(&selector["pattern"]);
    let name = if pattern.is_empty() || kind == "default" {
        kind
    } else {
        format!("{kind}: {pattern}")
    };
    let number = value["build_number"].as_u64().unwrap_or_default();
    Run {
        id: text(&value["uuid"]),
        number,
        name,
        title: text(&value["target"]["commit"]["message"])
            .lines()
            .next()
            .unwrap_or_default()
            .to_string(),
        event: text(&value["trigger"]["name"]).to_lowercase(),
        branch: text(&value["target"]["ref_name"]),
        status: status.into(),
        url: format!("https://bitbucket.org/{slug}/pipelines/results/{number}"),
        started: text(&value["created_on"]),
        finished: value["completed_on"].as_str().map(str::to_string),
        author: text(&value["creator"]["display_name"]),
        author_avatar: text(&value["creator"]["links"]["avatar"]["href"]),
    }
}

pub fn runs(repo: &Repo) -> Result<Vec<Run>, String> {
    let list = repo
        .get(
            &repo.path("/pipelines"),
            &[
                ("sort", "-created_on"),
                ("pagelen", "20"),
                ("fields", RUN_FIELDS),
            ],
        )
        .map_err(|e| {
            if e.contains("answered 403") {
                api::no_pipeline_scope()
            } else {
                e
            }
        })?;
    let slug = repo.slug();
    Ok(list["values"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| run_of(value, &slug))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_remarks_on_lines_and_on_the_whole_pull_request() {
        let on_line = json!({ "id": 7, "content": { "raw": "Rename this" }, "user": { "display_name": "Amandine T.", "links": { "avatar": { "href": "https://a/at.png" } } }, "inline": { "path": "src/A.php", "from": null, "to": 12, "src_rev": "a1b2c3d" }, "resolution": { "type": "comment_resolution" }, "created_on": "2026-10-23T09:00:00Z" });
        let removed = json!({ "id": 8, "content": { "raw": "Why?" }, "user": { "display_name": "A" }, "inline": { "path": "src/A.php", "from": 4, "to": null }, "parent": { "id": 7 }, "created_on": "t" });
        let outdated = json!({ "id": 9, "content": { "raw": "Old" }, "user": { "display_name": "A" }, "inline": { "path": "src/A.php", "to": 3, "outdated": true }, "created_on": "t" });
        let general = json!({ "id": 10, "content": { "raw": "LGTM" }, "user": { "display_name": "A" }, "created_on": "t" });
        let gone = json!({ "id": 11, "deleted": true });
        let a = remark_of(&on_line).unwrap();
        assert_eq!(
            (
                a.id.as_str(),
                a.body.as_str(),
                a.path.as_deref(),
                a.line,
                a.removed,
                a.author.as_str(),
                a.avatar.as_str()
            ),
            (
                "7",
                "Rename this",
                Some("src/A.php"),
                Some(12),
                false,
                "Amandine T.",
                "https://a/at.png"
            )
        );
        let b = remark_of(&removed).unwrap();
        assert_eq!(
            (b.line, b.removed, b.reply_to.as_deref()),
            (Some(4), true, Some("7"))
        );
        assert_eq!(remark_of(&outdated).unwrap().line, None);
        assert_eq!(remark_of(&general).unwrap().path, None);
        assert_eq!(remark_of(&gone), None);
        assert_eq!(
            (a.resolved, a.commit.as_deref(), b.resolved),
            (Some(true), Some("a1b2c3d"), None)
        );
        assert_eq!(remark_of(&general).unwrap().resolved, Some(false));
        assert_eq!(
            (a.kind.as_str(), remark_of(&general).unwrap().kind.as_str()),
            ("line", "general")
        );
    }

    #[test]
    fn reads_who_you_are() {
        let user = json!({ "display_name": "Quentin Briais", "links": { "avatar": { "href": "https://a/q.png" } } });
        assert_eq!(
            person(&user),
            Person {
                name: "Quentin Briais".into(),
                avatar: "https://a/q.png".into()
            }
        );
    }

    #[test]
    fn sums_up_bitbucket_runs() {
        let run = run_of(
            &json!({"uuid": "{x}", "build_number": 42, "state": {"name": "COMPLETED", "result": {"name": "STOPPED"}}, "target": {"ref_name": "main", "selector": {"type": "custom", "pattern": "deploy-prod"}, "commit": {"message": "Ship it\n\nmore"}}, "trigger": {"name": "MANUAL"}, "creator": {"display_name": "Quentin Briais", "links": {"avatar": {"href": "https://avatar/q"}}}, "created_on": "a", "completed_on": "b"}),
            "akki-team/fileas",
        );
        assert_eq!(
            (
                run.id.as_str(),
                run.name.as_str(),
                run.title.as_str(),
                run.status.as_str(),
                run.event.as_str(),
                run.url.as_str()
            ),
            (
                "{x}",
                "custom: deploy-prod",
                "Ship it",
                "cancelled",
                "manual",
                "https://bitbucket.org/akki-team/fileas/pipelines/results/42"
            )
        );
        assert_eq!(
            (
                run.author.as_str(),
                run.author_avatar.as_str(),
                run.started.as_str(),
                run.finished.as_deref()
            ),
            ("Quentin Briais", "https://avatar/q", "a", Some("b"))
        );
        let run = run_of(
            &json!({"state": {"name": "IN_PROGRESS", "stage": {"name": "RUNNING"}}, "target": {"selector": {"type": "default"}}}),
            "a/b",
        );
        assert_eq!(
            (run.name.as_str(), run.status.as_str(), run.finished),
            ("default", "running", None)
        );
        let paused = run_of(
            &json!({"state": {"name": "IN_PROGRESS", "stage": {"name": "PAUSED"}}}),
            "a/b",
        );
        assert_eq!(paused.status, "queued");
        let failed = run_of(
            &json!({"state": {"name": "COMPLETED", "result": {"name": "FAILED"}}}),
            "a/b",
        );
        assert_eq!(failed.status, "failure");
    }

    #[test]
    fn escapes_the_branch_in_the_query() {
        assert_eq!(
            branch_query(r#"feat/"x"\y"#),
            r#"source.branch.name="feat/\"x\"\\y""#
        );
    }

    #[test]
    fn sums_the_diffstat() {
        let stat = json!({ "values": [{ "lines_added": 10, "lines_removed": 2 }, { "lines_added": 2, "lines_removed": 1 }] });
        assert_eq!(diffstat(Some(&stat)), (Some(12), Some(3), Some(2)));
        assert_eq!(diffstat(None), (None, None, None));
    }

    #[test]
    fn sorts_pull_requests_into_the_three_lists() {
        let pr = |id: u64| json!({ "id": id, "state": "OPEN" });
        let (review, mine, others) = sorted(
            [vec![pr(1), pr(2)], vec![pr(3)], (1..=5).map(pr).collect()],
            "{me}",
        );
        let numbers = |list: &[Pull]| list.iter().map(|p| p.number).collect::<Vec<_>>();
        assert_eq!(numbers(&review), [1, 2]);
        assert_eq!(mine.len(), 1);
        assert_eq!(numbers(&others), [4, 5]);
    }

    #[test]
    fn reads_the_workspace_and_name_of_a_repo() {
        let groups = BTreeMap::from([
            ("owner".to_string(), "akki-team".to_string()),
            ("name".to_string(), "fileas".to_string()),
        ]);
        assert_eq!(
            owner_and_name("x", &groups).unwrap(),
            ("akki-team".into(), "fileas".into())
        );
        assert_eq!(
            owner_and_name("a/b", &BTreeMap::new()).unwrap(),
            ("a".into(), "b".into())
        );
        assert!(owner_and_name("a/b/c", &BTreeMap::new()).is_err());
    }
}
