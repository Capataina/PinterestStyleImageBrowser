---
source_type: shipped-project
date_published: 2026-04
hype_score: 1
---

# TanStack Query — Optimistic Updates / Mutations Pattern

## Source reference

- TanStack Query Optimistic Updates: https://tanstack.com/query/v4/docs/react/guides/optimistic-updates
- TanStack Query v5 Optimistic Updates: https://tanstack.com/query/v5/docs/framework/solid/guides/optimistic-updates
- Discussion #2734 (sequential rollback): https://github.com/TanStack/query/discussions/2734

## Claim summary

Canonical optimistic-update pattern in TanStack Query: `onMutate` cancels in-flight queries + snapshots cache + applies optimistic update + returns context; `onError` rolls back from context; `onSettled` invalidates to reconcile with server. Framework-agnostic (React, Vue, Solid, Angular).

## Relevance to our project

A3: The project's `context/notes/conventions.md` documents this pattern verbatim and the implementation in `useImages.ts` / `useTags.ts` is canonical. This is a credibility marker — the project follows the framework's idiom precisely.

## Specific takeaways

- The pattern in the project already includes the rollback context, optimistic placeholder for new tags (id=-1), and post-success swap (per `useTags.ts` `useCreateTag`).
- The "stale: Infinity" config + optimistic mutations is the right combination for a desktop-app where the network is local IPC (~1ms).

## Hype indicators

None — official docs.
