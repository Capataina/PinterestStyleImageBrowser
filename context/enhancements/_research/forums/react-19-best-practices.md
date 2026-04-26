---
source_type: forum-blog
date_published: 2025-12
hype_score: 4
---

# React 19 — Compiler, Server Components, Concurrent Rendering Best Practices

## Source reference

- React.dev v19 release: https://react.dev/blog/2024/12/05/react-19
- "React 19 Best Practices" (DEV.to): https://dev.to/jay_sarvaiya_reactjs/react-19-best-practices-write-clean-modern-and-efficient-react-code-1beb
- React Performance Optimization 2025: https://www.growin.com/blog/react-performance-optimization-2025/

## Claim summary

React 19 introduced: React Compiler (auto-memoisation, replaces `useMemo` / `useCallback` largely), Server Components (server-rendered, RSC streaming), Concurrent Rendering by default (interruptible), Actions API (useActionState), useOptimistic hook (formal optimistic-update pattern).

## Relevance to our project

A3: The project is on React 19. The `useOptimistic` hook is a formal-React-19 pattern that the project's existing TanStack Query optimistic mutations could be modernised against (TanStack Query already does this; the question is alignment + reduced custom code).

Other React 19 features (Server Components, Actions) are less applicable to a Tauri app — there's no server. So the specific React 19 wins for this project are: compiler (free), `useOptimistic` (alignment with framework), concurrent rendering (already on by default).

## Specific takeaways

- The React Compiler can be incrementally enabled component-by-component.
- `useOptimistic` is the React 19 formalism for optimistic updates; the project's pattern (per `context/notes/conventions.md`) already does this manually via TanStack Query.
- For Tauri apps the Server Components story is moot.

## Hype indicators

Moderate — DEV.to and Growin posts are SEO-shaped but the underlying React 19 documentation is authoritative.
