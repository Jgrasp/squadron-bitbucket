use serde_json::{Value, json};
use squadron_extension_api::forge::{
    self, BranchArgs, Input, NoArgs, NumberArgs, PostArgs, ReplyArgs, ResolveArgs, UpdateArgs,
};
use squadron_extension_api::{Account, Extension, host, squadron_extension};

mod api;
mod pulls;
mod reads;
mod writes;

use reads::Repo;

struct Bitbucket;

fn on_repo<A, T>(
    input: &str,
    work: impl Fn(&Repo, &A) -> Result<T, String>,
) -> Result<String, String>
where
    A: Default + serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    let asked: Input<A> = forge::input(input)?;
    let (owner, name) = reads::owner_and_name(&asked.repo, &asked.groups)?;
    let key = format!("workspace:{}", owner.to_lowercase());
    let known = host::state_get(&key);
    let mut last = String::new();
    for account in api::ordered(api::accounts()?, known.clone()) {
        let repo = Repo {
            account: account.clone(),
            owner: owner.clone(),
            name: name.clone(),
        };
        match work(&repo, &asked.args) {
            Ok(found) => {
                if known.as_deref() != Some(&account) {
                    host::state_set(&key, Some(&account));
                }
                return forge::answer(&found);
            }
            Err(error) if api::denied(&error) => {
                if last.is_empty() || !api::scoped(&last) {
                    last = error;
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

fn checked(status: u16, body: &str) -> Result<String, String> {
    match status {
        200 => {
            let user: Value = serde_json::from_str(body).unwrap_or_default();
            let name = user["display_name"].as_str().unwrap_or_default();
            Ok(json!({
                "text": { "en": format!("Connected as {name}"), "fr": format!("Connecté en tant que {name}") }
            })
            .to_string())
        }
        401 => Err(json!({
            "title": { "en": "Bitbucket refused this email and token", "fr": "Bitbucket a refusé cet e-mail et ce jeton" },
            "detail": { "en": "The email or the token may be wrong, or the token expired or revoked.", "fr": "L’e-mail ou le jeton est peut-être faux, ou le jeton expiré ou révoqué." },
            "field": "token"
        })
        .to_string()),
        403 => Ok(json!({
            "text": { "en": "Connected, without the read:user:bitbucket scope", "fr": "Connecté, sans le scope read:user:bitbucket" },
            "detail": {
                "en": "Squadron cannot list the pull requests you review or wrote. Create a token with the five scopes.",
                "fr": "Squadron ne peut pas lister les pull requests que vous relisez ou avez écrites. Créez un jeton avec les cinq scopes."
            }
        })
        .to_string()),
        status => Err(format!("Bitbucket answered {status}.")),
    }
}

impl Extension for Bitbucket {
    fn check_account(account: Account) -> Result<String, String> {
        let me = api::get(&account.id, "/user", &[("fields", "display_name")])?;
        checked(me.status, &me.body)
    }

    fn forge(call: String, input: String) -> Result<String, String> {
        match call.as_str() {
            "pull" => on_repo(&input, |repo, args: &BranchArgs| {
                reads::pull(repo, &args.branch)
            }),
            "numbered" => on_repo(&input, |repo, args: &NumberArgs| {
                reads::numbered(repo, args.number)
            }),
            "lists" => on_repo(&input, |repo, _: &NoArgs| reads::lists(repo)),
            "remarks" => on_repo(&input, |repo, args: &NumberArgs| {
                reads::remarks(repo, args.number)
            }),
            "me" => on_repo(&input, |repo, _: &NoArgs| reads::me(repo)),
            "runs" => on_repo(&input, |repo, _: &NoArgs| reads::runs(repo)),
            "reply" => on_repo(&input, |repo, args: &ReplyArgs| writes::reply(repo, args)),
            "resolve" => on_repo(&input, |repo, args: &ResolveArgs| {
                writes::resolve(repo, args)
            }),
            "post" => on_repo(&input, |repo, args: &PostArgs| writes::post(repo, args)),
            "merge" => on_repo(&input, |repo, args: &NumberArgs| {
                writes::merge(repo, args.number)
            }),
            "update" => on_repo(&input, |repo, args: &UpdateArgs| writes::update(repo, args)),
            _ => Err(forge::unsupported(&call)),
        }
    }
}

squadron_extension!(Bitbucket);

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn manifest() -> Value {
        serde_json::from_str(include_str!("../extension.json")).unwrap()
    }

    fn patterns(key: &str) -> Vec<Regex> {
        manifest()["contributes"]["forge"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Regex::new(p.as_str().unwrap()).unwrap())
            .collect()
    }

    fn repo(found: &regex::Captures) -> String {
        format!("{}/{}", &found["owner"], &found["name"])
    }

    #[test]
    fn recognizes_bitbucket_remotes() {
        let remotes = patterns("remotes");
        let matched = |remote: &str| {
            remotes
                .iter()
                .find_map(|r| r.captures(remote).map(|c| repo(&c)))
        };
        assert_eq!(
            matched("bitbucket.org/akki-team/fileas").as_deref(),
            Some("akki-team/fileas")
        );
        assert_eq!(
            matched("ssh.bitbucket.org/akki-team/fileas").as_deref(),
            Some("akki-team/fileas")
        );
        assert_eq!(matched("github.com/Jgrasp/squadron"), None);
        assert_eq!(matched("bitbucket.org/a/b/c"), None);
        assert_eq!(matched("notbitbucket.org/a/b"), None);
    }

    #[test]
    fn recognizes_pull_request_links() {
        let links = patterns("links");
        let anchored: Vec<Regex> = links
            .iter()
            .map(|r| Regex::new(&format!("^{}([/?#].*)?$", r.as_str())).unwrap())
            .collect();
        let matched = |url: &str| {
            anchored
                .iter()
                .find_map(|r| r.captures(url).map(|c| (repo(&c), c["number"].to_string())))
        };
        assert_eq!(
            matched("https://bitbucket.org/steel-shed/bmc/pull-requests/412/overview"),
            Some(("steel-shed/bmc".into(), "412".into()))
        );
        assert_eq!(
            matched("https://bitbucket.org/akki-team/fileas/pull-requests/2108"),
            Some(("akki-team/fileas".into(), "2108".into()))
        );
        assert_eq!(
            matched("https://bitbucket.org/steel-shed/bmc/pull/412"),
            None
        );
        assert_eq!(matched("https://github.com/agence/orbit/pull/2108"), None);
    }

    #[test]
    fn labels_every_text_in_english_and_french() {
        let found = manifest();
        let account = &found["contributes"]["accounts"];
        let mut texts = vec![&found["name"], &found["description"], &account["label"]];
        for field in account["fields"].as_array().unwrap() {
            texts.push(&field["label"]);
            texts.push(&field["hint"]);
        }
        for text in texts {
            assert!(text["en"].is_string() && text["fr"].is_string(), "{text}");
        }
    }

    #[test]
    fn tells_whether_an_account_works() {
        let ok: Value =
            serde_json::from_str(&checked(200, r#"{"display_name":"Quentin Briais"}"#).unwrap())
                .unwrap();
        assert_eq!(ok["text"]["en"], "Connected as Quentin Briais");
        let refused: Value = serde_json::from_str(&checked(401, "").unwrap_err()).unwrap();
        assert_eq!(refused["field"], "token");
        let scoped: Value = serde_json::from_str(&checked(403, "").unwrap()).unwrap();
        assert!(scoped["detail"]["en"].is_string());
        assert_eq!(checked(500, "").unwrap_err(), "Bitbucket answered 500.");
    }
}
