# Bitbucket

A [Squadron](https://github.com/Jgrasp/squadron) extension that brings the pull requests and Pipelines runs of your Bitbucket Cloud projects into Squadron.

## Use

In Settings › Extensions › Bitbucket, add an account with the email of your Atlassian account and an [API token with scopes](https://id.atlassian.com/manage-profile/security/api-tokens): choose **Create API token with scopes**, name it Squadron, pick **Bitbucket**, then tick these scopes:

- `read:repository:bitbucket`
- `read:pullrequest:bitbucket`
- `write:pullrequest:bitbucket`
- `read:user:bitbucket`
- `read:pipeline:bitbucket`

**Test the connection** checks the token before you save it. Add one account per Atlassian user you work as: for each workspace, the extension remembers the account that reached it.

Every project whose `origin` is on `bitbucket.org` then gets, in Squadron:

- the pull request of each agent's branch, and chips for the pull request links in conversations;
- the pull requests to review, yours and the others, with their checks and reviewers;
- reviews with line comments, a summary and a verdict, replies to remarks, resolved threads, merges with the method allowed on the destination branch and the source branch closed or kept, and title and description changes, from you or from an agent;
- the Pipelines runs, with a notification when one ends.

## What it asks Squadron for

- HTTPS requests to `api.bitbucket.org`, Bitbucket Cloud's API.
- Bitbucket accounts: each token stays in the system keyring. Squadron adds the email and token to the requests to Bitbucket, and the extension never reads the token.
  - `read:repository:bitbucket` reads the repository and the commit a pull request was last changed on.
  - `read:pullrequest:bitbucket` and `write:pullrequest:bitbucket` read the pull requests, their comments and checks, and comment, approve, request changes, merge and edit them.
  - `read:user:bitbucket` tells which pull requests you review or wrote, and shows your name and avatar.
  - `read:pipeline:bitbucket` shows the Pipelines runs.

## Build

```sh
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
cargo test
```

CI checks the formatting of every push and pull request with `cargo fmt --check`.
