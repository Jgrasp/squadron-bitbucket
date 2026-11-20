# Changelog

## 0.2.0 · 2026-11-20

- Merges use the method set in Squadron's Settings › Pull requests when the destination branch allows it: merge commit, squash, or rebase
- The merge confirmation starts from the pull request's "Close source branch" box, and Bitbucket deletes the source branch, or keeps it, as you choose
- Merging no longer unticks "Close source branch" on a pull request where it was ticked after it was created
- Requires Squadron 0.56.0 or later

## 0.1.0 · 2026-11-19

- Pull requests of your Bitbucket Cloud projects in Squadron: the pull request of each agent's branch, the ones to review, yours and the others, with their checks and reviewers
- Reviews with line comments, a summary and a verdict, replies to remarks, resolved threads shown and resolved from Squadron, merges, and title and description changes
- Pipelines runs of each project, with a notification when one ends
- Pipelines run on a pull request show its number and title and are filed under PR checks
- Chips for Bitbucket pull request links in conversations
- Bitbucket accounts with an email and an API token checked before it is saved, one per Atlassian user
