# bard — blueprint library

## §0 Using This Blueprint

**Scope.** This index names the documents bard is built from, records which
document owns which class of fact, and states the conventions every document in
the library follows. It carries no product facts of its own.

**Deliverables.** The library is complete enough that a team who has never met
the author can build bard from it: a paid native Windows application that
merges a capture and clipboard workspace with local-first dictation and an
explicitly opt-in cloud cleanup stage.

**Path conventions.** Repository paths are relative to `bard/` — the product
repository. `src/` is the webview, `src-tauri/` the Rust core, `docs/` the
written record. Paths outside the repository are absolute and named as such.

**Cross-reference convention.** Every reference to another document carries
that document's name, as in `Design System §4`. A bare marker such as §3
means a section of the document being read.

**Status key.** Entries carry no build status. Where a capability belongs to a
later stage of the specification it is marked **Phase 2** or **Phase 3**, which
is a statement about scope rather than progress. Phase 1 is the shipping
product; Phase 2 completes parity with the two reference products; Phase 3 is
everything beyond them.

**Secrets policy.** No credential, key or token value appears anywhere in this
library. Names appear; values live in the operator's own store and are named at
the point of use in `Technology §8`.

**Anchor policy.** Facts owned by outside parties — framework versions,
provider quotas, model licences, API shapes — are named, linked and bounded by
the constraint bard depends on, never transcribed. Resolved values live in
`src-tauri/Cargo.toml`, `package.json` and their lockfiles.

---

## §1 The documents

| # | Document | Owns |
|---|---|---|
| 01 | **Product & Business** | What bard is, who it is for, the market it sits in, every behaviour of the product, the data schema, pricing, licensing, distribution, legal position |
| 02 | **Technology** | The stack, the window architecture, the transcription engine, the model catalogue contract, model discovery/import and OpenRouter integration, the build and release pipeline |
| 03 | **Design System** | Philosophy, tokens, type, space, shape, motion, themes, components, accessibility, anti-patterns — everything true of all surfaces |
| 04 | **Surfaces** | Every window and overlay, specified to build-readiness — everything true of one surface |
| 05 | **Input & Commands** | Global shortcuts, the capture mechanism, the tray, dictation triggers, paste behaviour — the command surface of a keyboard-first application |

**Two of the canonical seven are absent, deliberately.** bard sends no email,
so there is no Email Templates document; any message it shows a person is copy,
owned by `Surfaces`. Its public documentation is a product page on miiso.dev
rather than a help centre, and is specified in `Product & Business §9` rather
than given a document — a single page does not need an information
architecture.

---

## §2 Ownership map

The table that keeps the library coherent. A fact appears in full in exactly
one document; every other mention is a reference.

| Class of fact | Owner | Own or anchor |
|---|---|---|
| What bard is, and the tests it must pass | Product & Business | Own |
| Competitive position, reference products | Product & Business | Own |
| Feature behaviour — what happens when | Product & Business | Own |
| SQLite schema and migrations | Product & Business | Own |
| Price, licence terms, activation model, retention policy | Product & Business | Own |
| Licence-service API, signed-lease transport, Explorer registration | Technology | Own integration; anchor external API |
| Distribution channels and update policy | Product & Business | Own |
| Third-party licence obligations | Product & Business | Own |
| Model licences and their terms | Product & Business | Anchor |
| Framework, runtime and crate versions | Technology | Anchor |
| Window architecture, companion docking and lifecycle | Technology | Own |
| Transcription engine and the architectures it reads | Technology | Own |
| Model catalogue schema and semantics | Technology | Own |
| Model files — URL, revision, checksum | Technology | Anchor (linked and pinned) |
| Hugging Face API shapes | Technology | Anchor |
| Build prerequisites and their traps | Technology | Own |
| Design tokens, type scale, motion values, audio-cue palette | Design System | Own |
| Component anatomy, variants, states | Design System | Own |
| Accessibility conformance targets | Design System | Own |
| Per-surface layout and zones | Surfaces | Own |
| Exact user-facing copy | Surfaces | Own |
| Per-surface failure modes | Surfaces | Own |
| Global shortcuts and their defaults | Input & Commands | Own |
| Capture mechanics, clipboard handling and Explorer handoff | Input & Commands | Own |
| Tray menu and its actions | Input & Commands | Own |
| Implementation status and production-gate evidence | `docs/PARITY.md`, outside this library | Own |

**Where two documents could claim a fact**, three questions resolve it: which
document would a reader open first looking for it; which document breaks if it
changes; which already owns the surrounding facts.

---

## §3 The standard every document is held to

**Present tense, third person, no addressee.** bard exists; the documents
describe it. No author, no team, no reader being instructed.

**One canonical home per fact.** A rule stated twice diverges. The second copy
is the one that goes stale, and nothing tells a reader which is current.

**The blueprint decides.** No document carries an open question, a "to be
confirmed", or a decision handed to whoever implements the surface. Where a
constraint comes from outside, the document specifies the design that accounts
for it rather than reporting the conflict.

**Failure is specified, not only success.** Every surface entry carries a
*Fails when*; every integration carries what bard does when the other side does
not answer. A specification that describes only the happy path is a
description.

**External facts are anchored.** A transcribed version number or provider quota
reads as more authoritative than a link and is the opposite: it decays with
nothing to announce it.

---

*Library version 1.3 · five documents*
