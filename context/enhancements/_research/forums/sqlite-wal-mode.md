---
source_type: forum-blog
date_published: 2024-07
hype_score: 1
---

# SQLite WAL Mode — Concurrency, Performance, Single-Writer Limitation

## Source reference

- SQLite docs: https://sqlite.org/wal.html
- Oldmoe's blog: https://oldmoe.blog/2024/07/08/the-write-stuff-concurrent-write-transactions-in-sqlite/
- Tenthousandmeters: https://tenthousandmeters.com/blog/sqlite-concurrent-writes-and-database-is-locked-errors/

## Claim summary

WAL mode (`PRAGMA journal_mode=WAL`) lets readers and writers operate concurrently — readers don't block writers, writer doesn't block readers. Single-writer constraint remains. Throughput: 70k-100k write transactions per second for typical record sizes. 100+ concurrent writers can hit "database is locked" errors.

## Relevance to our project

A1 + A3: The project's vault `Decisions.md` D3 notes that WAL is *claimed* in old memory-bank docs but not actually enabled. For a single-user desktop app the practical benefit is small (only one process), but it's a defensible-low-cost addition: one `PRAGMA` line + better behaviour if any future feature ever spawns parallel SQL access.

The recommendation downstream: enable WAL mode for hygiene + future-proofing, even if no current code path benefits. Plus, document the choice rather than leaving the gap.

## Specific takeaways

- One-line change at `ImageDatabase::new`: `connection.execute_batch("PRAGMA journal_mode=WAL")`.
- Side effect: a `images.db-wal` and `images.db-shm` file appear next to the DB. Worth documenting.
- Reduces risk of deadlock in any future rayon-parallelised SQL access.

## Hype indicators

Mild (some 3rd-party blogs are SEO-shaped) but the underlying SQLite docs are authoritative.
