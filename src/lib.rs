use serde_json::{Value, json};
use squadron_extension_api::forge;
use squadron_extension_api::{Account, Extension, squadron_extension};

mod api;

struct Bitbucket;

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

    fn forge(call: String, _input: String) -> Result<String, String> {
        Err(forge::unsupported(&call))
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
