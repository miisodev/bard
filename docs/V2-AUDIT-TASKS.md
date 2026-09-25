# V2 production audit — task list

Durable list for the batch mandated 2026-09-25. Work in order. Mark each item
`done` or leave it `open` with a reason. Nothing public or irreversible ships
without Miiso.

**Mandate.** Comprehensive production audit across code quality, security,
UI/UX, keyboard usage, operational surfaces, stress, performance, aesthetics,
user friendliness, positioning, branding, representation. Standard applied to
every artefact without exception: *does this embody a minimal modern aesthetic,
and would Apple ship it as it stands?*

**Constraints.**
- Commit freely. **Do not push.** The Microsoft Store listing is the gate.
- Bard V2 must run locally so it can be used and refined in real conditions.
- No feature creep. Every change traces to a row in `docs/PARITY.md`.

---

## 0. Session state

| Item | State |
| --- | --- |
| Dev build on machine | done — `target/debug/bard.exe`, PID 46900, data dir shared with 0.1.2 |
| Input-death fix | done — `edd4e93`, committed locally, **not pushed** |
| Input stack provably alive | done — `input:listener-started`, `input:alive:2` |
| Rust suite | done — 63 tests pass |
| Frontend gate | done — tsc clean, css-audit clean (414 rules, 86 tokens) |
| Release 0.1.2 stopped | done — replaced by dev build; log snapshotted to `%TEMP%\opencode\bard-lifecycle-evidence.log` |

## 1. Audit lanes (all read-only, all returned)

| Lane | Session | Status |
| --- | --- | --- |
| Security & privacy | `ses_f25dbfd89ffeVdQcc1iW7F6v8y` | done — 14 findings, 0 critical/high, 3 medium |
| Keyboard & input | `ses_f25dbfd85ffeniJDwZj1YY9q0Z` | done |
| UI, aesthetics, brand | `ses_f25dba505ffeb85C9OliNZ3wf3` | done |
| Code quality, perf, stress | `ses_f25dba504ffeiGUOiHi43OV7pP` | done — 48 findings, 4 critical, 12 high |
| Product & fidelity | `ses_f25db56e9ffe5Fzcl2IjEqiPyi` | done — 40 findings, 5 critical |

## 2. Blocking defects — a user is blocked or loses data

Ordered by "does this stop someone using the product".

| # | Item | Where | State |
| --- | --- | --- | --- |
| B1 | Store copy cannot complete first run — `setAutostart` rejects unconditionally, `onDone()` never runs | `Onboarding.tsx:124`, `commands.rs:1076` | open |
| B2 | Composer never takes focus; typing does nothing while `focus-within` makes it look ready | `Panel.tsx:640` | open |
| B3 | No skip on the licence step — the free path is gated behind a red error | `Onboarding.tsx:121` | open |
| B4 | `machine_id()` overwrites the installation ID on a credential-read timeout, destroying a paying customer's binding irrecoverably | `license.rs:266-295` | open |
| B5 | Dictation retention defaults to 7 days in Rust, 30 in UI and blueprint — transcripts deleted 23 days early | `db.rs:783` vs `Data.tsx:16` | open |
| B6 | Draw discards unsaved work silently — no dirty marker, no confirmation | `DrawWindow.tsx:265,381` | open |
| B7 | Draw `save()` has no `catch` — a failed write loses the drawing with no message | `DrawWindow.tsx:227` | open |
| B8 | "Clear history" deletes notes, drawings and favourites with no confirmation | `db.rs:862`, `Data.tsx:197` | open |
| B9 | Silent 5000-item cap overrides `Forever`, runs on every capture, disclosed nowhere | `monitor.rs:54`, `db.rs:841` | open |
| B10 | `clean_text` collapses 3+ identical characters — mangles `1000000`, `...`, `===` before storage | `dictation/mod.rs:363` | open |
| B11 | Every dictation written to the timeline even when dictated into another app — contradicts the privacy claim | `dictation/mod.rs:302` | open |
| B12 | Disk full aborts the process — `insert_*` unwraps on `execute` | `db.rs:336,438,495` | open |

## 3. Silent failure — the app lies or says nothing

| # | Item | Where | State |
| --- | --- | --- | --- |
| S1 | Copy reports "Copied" on a failed clipboard write | `Panel.tsx:786` | open |
| S2 | Capture failure is emitted and never consumed | `capture.rs:320` | open |
| S3 | Retention pass cannot report success or failure | `db.rs:768`, `commands.rs:922` | open |
| S4 | Export has no `.catch`; creates a folder it was told not to create | `Data.tsx:114`, `commands.rs:595` | open |
| S5 | `redo()` never repaints the canvas | `DrawWindow.tsx:211` | open |
| S6 | `setAutostart` rejection unhandled in General; toggle silently reverts | `General.tsx:82` | open |
| S7 | No React error boundary — a render throw is a blank frameless rectangle | `App.tsx:183` | open |
| S8 | Restricted users cannot release an activation; `clear_license` is called from nothing | `LicenseCard.tsx:114` | open |
| S9 | Updater key in the JS toolchain's environment — a transitive dep could sign a malicious installer | `build-release.ps1:76`, `release.yml:79` | open |
| S10 | Deleted text survives in `bard.db-wal` and freelist pages | `db.rs:114` | open |
| S11 | Machine binding is credential-store binding; both halves export together | `license.rs:262` | open |
| S12 | No redirect policy — a 307 replays the licence key to any host | `license.rs:615` | open |

## 4. Performance — will fail the stress fixture

| # | Item | Where | State |
| --- | --- | --- | --- |
| P1 | `get_state` = 3 unbounded reads + 5 credential round-trips, on every `refresh`, × 4 live webviews | `db.rs:1043` | open |
| P2 | O(items × attachments) filter per card, re-rendered at 20 Hz during a take | `Panel.tsx:558`, `App.tsx:141` | open |
| P3 | `mic-level` at 20 Hz re-renders the whole panel — 24 000 renders per take | `audio.rs:199` | open |
| P4 | Nothing bounds captured text or image bytes — one large payload bricks the panel | `monitor.rs:42` | open |
| P5 | ItemCard fetches every image as base64 eagerly, off-screen included | `ItemCard.tsx:55` | open |
| P6 | Opening Models auto-probes every detected model — a full load each, twice for one | `Models.tsx:368`, `hf.rs:1279` | open |
| P7 | `probe_model_path` releases the guard on timeout, so abandoned models accumulate | `hf.rs:604` | open |
| P8 | Six credential sites leak a thread per call under a Credential Manager stall | `license.rs:182+`, `cloud.rs:349+` | open |
| P9 | N+1 on delete; `list_items` called from 8 places with no `LIMIT` | `db.rs:810` | open |

## 5. Supervision — the class of defect just fixed, elsewhere

| # | Item | Where | State |
| --- | --- | --- | --- |
| W1 | Clipboard monitor loop: no heartbeat, no supervision, no record | `monitor.rs:68` | open |
| W2 | Licence revalidation loop: same — a dead loop means a revoked licence is never enforced | `license.rs:789` | open |
| W3 | `log::warn!` to a sink nobody reads | `capture.rs:133`, `overlay.rs:504` | open |
| W4 | A take has no overall watchdog; a stuck inference wedges dictation until restart | `dictation/mod.rs:234` | open |

## 6. Product, copy, trust

| # | Item | State |
| --- | --- | --- |
| C1 | Four different one-liners ship for one product | open |
| C2 | "Shift Shift" taught without a side in 3 surfaces — Right Shift opens the panel, so users tap wrong and conclude it's broken | open |
| C3 | Onboarding step 1 leads with the brand name, not the value; step 2 is a 202 MB download before any value | open |
| C4 | "annual licence key" — subscription language on a product that never renews | open |
| C5 | Shortcut sheet omits Capture and Dictate — the two gestures the product exists for | open |
| C6 | Three-machine limit and 365-day term absent from the product | open |
| C7 | README claims a signed installer and renewal via an "account flow" — both false | open |
| C8 | Permanent system-wide `WH_MOUSE_LL` hook installed at launch, never disclosed | open |
| C9 | No `Couldn't copy.` anywhere; no copy path has a `.catch` | open |
| C10 | Errors auto-dismiss in 2.6 s with no dismiss control | open |
| C11 | Every card is a Tab stop (500 items = 500 stops); no `aria-activedescendant` | open |
| C12 | No store listing, no screenshot manifest, no adaptive icon | open |
| C13 | `PARITY.md` stale: command count, A11, D8, D11, B7, E10, E4, LOC | open |

## 7. Infrastructure

| # | Item | State |
| --- | --- | --- |
| I1 | No frontend test runner at all | open |
| I2 | No 500-item stress fixture; `drive.ps1` is a manual driver | open |
| I3 | No documented performance budgets — gate G10 is unfalsifiable | open |
| I4 | No orphan-image sweep; no WAL checkpoint outside full erase | open |
| I5 | No `cargo audit` in CI | open |
| I6 | `clear_license` exposed but unreachable | open |

## 8. Verification still owed

- [ ] Reproduce the original input death on a real restart (Settings → Draw) — the fix is in, the trigger is not yet known.
- [ ] `window.confirm` / `window.prompt` behaviour in WebView2 — if unimplemented, the destructive gate is ungated and the Draw text tool is dead. Five minutes with the app running.
- [ ] Every finding marked "could not verify" by any lane.
- [ ] Screenshot every surface — no lane has seen the product. `scripts/shot.ps1`.

## 9. Definition of done for this batch

- Every B-item fixed and verified on the running dev build.
- Every S-item either fixed or consciously recorded in `PARITY.md` as accepted.
- P1–P3 fixed, then the 500-item fixture measured against written budgets.
- W1–W2 given the same treatment as the input listener.
- Frontend runner exists with at least one test file.
- All surfaces looked at, not just read.
- Committed locally. **Not pushed.**
