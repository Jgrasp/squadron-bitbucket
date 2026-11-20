# Journal des modifications

## 0.3.0 · 2026-11-20

- Ouvrez une pull request sur Bitbucket depuis la branche poussée d’un agent, avec son titre, sa description et son état brouillon
- La nouvelle pull request reçoit les relecteurs par défaut du dépôt, comme quand vous l’ouvrez sur Bitbucket
- Un message clair quand la branche n’a aucun changement à fusionner ou a déjà une pull request ouverte

## 0.2.0 · 2026-11-20

- Les fusions utilisent la méthode choisie dans Réglages › Pull requests de Squadron quand la branche de destination l’autorise : commit de fusion, squash ou rebase
- La confirmation de fusion part de la case « Fermer la branche source » de la pull request, et Bitbucket supprime la branche source, ou la garde, selon votre choix
- Fusionner ne décoche plus « Fermer la branche source » sur une pull request où elle a été cochée après sa création
- Nécessite Squadron 0.56.0 ou plus récent

## 0.1.0 · 2026-11-19

- Les pull requests de vos projets Bitbucket Cloud dans Squadron : celle de la branche de chaque agent, celles à relire, les vôtres et les autres, avec leurs checks et leurs relecteurs
- Des revues avec commentaires de ligne, résumé et verdict, des réponses aux remarques, des fils résolus affichés et résolus depuis Squadron, des fusions, et le changement du titre et de la description
- Les runs Pipelines de chaque projet, avec une notification à la fin de chacun
- Les pipelines lancés sur une pull request affichent son numéro et son titre et sont rangés dans les checks de PR
- Des puces pour les liens de pull requests Bitbucket dans les conversations
- Des comptes Bitbucket avec un e-mail et un jeton d’API vérifié avant d’être enregistré, un par utilisateur Atlassian
