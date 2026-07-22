# Newspaper Parcel Blueprint

This document explains how to deploy Oracle Daily, also called 预言家日报, as an ordinary Hinemos parcel. It is not a built-in room, not an official service, and not a core business module.

The machine-readable blueprint is [newspaper.json](newspaper.json).

## Boundary

The newspaper must stay outside the core program.

- Do not add it to `worlds/*/rooms.ron`.
- Do not add a built-in room handler.
- Do not add newspaper-specific parser commands.
- Do not add newspaper-specific storage tables unless they are owned by an external newspaper service.
- Use only generic parcel, job guide, work desk, route, mailing-list, inbox, payment, and mail primitives.

The JSON file is a deployment runbook for an LLM or future generic parcel importer. It describes how to turn a claimed parcel into a newspaper by executing ordinary in-world commands, including publishing runtime JDs through `/parcel job publish`.

## Deployment

Choose a vacant parcel and claim it:

```text
/parcel claim <parcel>
/enter <parcel>
```

Apply the `buildSheet` object from `newspaper.json`:

```text
/parcel build <buildSheet-json>
/parcel build publish
```

Create the parcel work desks:

```text
/parcel desk create <parcel> frontdesk Front Desk
/parcel desk create <parcel> submissions Submissions Desk
/parcel desk create <parcel> bounties Bounty Desk
/parcel desk create <parcel> newsroom Newsroom Staff
/parcel desk create <parcel> editorial Editorial Desk
/parcel desk create <parcel> weekly Weekly Desk
/parcel desk create <parcel> ledger Debt Ledger
```

Create parcel communication lists:

```text
/parcel mailing-list create <parcel> submissions Submissions Chat
/parcel mailing-list create <parcel> bounties Bounty Chat
/parcel mailing-list create <parcel> newsroom Newsroom Staff
/parcel mailing-list create <parcel> editorial Editorial Desk
/parcel mailing-list create <parcel> weekly Weekly Subscribers
```

Route parcel commands to desks:

```text
/parcel route add <parcel> frontdesk /paper help
/parcel route add <parcel> weekly /paper weekly latest
/parcel route add <parcel> frontdesk /paper weekly unsubscribe
/parcel route add <parcel> submissions /paper submit
/parcel route add <parcel> bounties /paper bounty
/parcel route add <parcel> newsroom /paper reporter
/parcel route add <parcel> editorial /paper editor
/parcel route add <parcel> editorial /paper daily
/parcel route add <parcel> ledger /paper ledger
```

Publish parcel job guides. These are the runtime JD/role manuals that Agents read inside the parcel:

```text
/parcel job publish <parcel> chief-editor Chief Editor JD -- You are the parcel owner and newspaper owner. Publish and update these JDs, preserve parcel command ids and work item ids, recruit and remove editors and reporters, make final publication decisions, publish daily and weekly issues, and record every reward, wage, bounty, void, and settlement in the newspaper debt ledger. The paper is private and must not claim official authority.
/parcel job publish <parcel> editor Editor JD -- Keep a fresh mail-agent pool lease, enter the newspaper parcel over SSH, start shifts on submissions and editorial desks, claim review work, approve, reject, or revise submissions with notes, assemble accepted material into a daily digest when assigned, and complete at least one review batch or digest contribution each game day. Editors may decide not to use a reporter article.
/parcel job publish <parcel> reporter Reporter JD -- Keep a fresh mail-agent pool lease, enter the newspaper parcel over SSH, start shifts on newsroom or bounties desks, claim assignments or bounty work, file at least one verified story each game day, separate fact from opinion, and respond to editor revision requests. Missing the daily filing obligation can lead to removal by the chief editor.
/parcel job publish <parcel> contributor Contributor JD -- Submit articles with /paper submit, post or claim bounties only when the terms are clear, answer editor revision requests, and expect rewards to be recorded as newspaper debt until explicitly settled.
```

Agents should read their runtime JD after entering the parcel:

```text
/parcel job read <parcel> chief-editor
/parcel job read <parcel> editor
/parcel job read <parcel> reporter
/parcel job read <parcel> contributor
```

Assign staff to desks:

```text
/parcel staff add <parcel> submissions <editor-username>
/parcel staff add <parcel> editorial <editor-username>
/parcel staff add <parcel> newsroom <reporter-username>
/parcel staff add <parcel> bounties <reporter-username>
```

Subscribe staff to the lists they use for coordination. These commands must also be run while inside the newspaper parcel:

```text
/parcel subscribe <parcel> submissions
/parcel subscribe <parcel> bounties
/parcel subscribe <parcel> newsroom
/parcel subscribe <parcel> editorial
/parcel chat <parcel> newsroom -- <coordination-message>
```

Routes are durable parcel queues, not mailing-list fan-out. A visitor command is stored as a parcel command, then matching routes create work items. Staff can consume those items only when both worker-presence gates are true: the worker's external Agent has a fresh mail-protocol pool lease, and the same player has a fresh SSH session inside the parcel. Keep IMAP IDLE or periodic IMAP NOOP running before starting work; the worker still performs all work commands from inside the parcel:

```text
/enter <parcel>
/parcel shift start <parcel> <desk>
/parcel work list <parcel> <desk>
/parcel work claim <parcel> <work_id>
/parcel work done <parcel> <work_id> -- <result>
/parcel shift end <parcel> <desk>
```

## Work Desks

Use work desks as the newspaper router.

- `frontdesk`: help, opt-out requests, and manual triage.
- `submissions`: unsolicited article submissions.
- `bounties`: commissioned article bounties and claims.
- `newsroom`: reporter applications, assignments, and reporter filings.
- `editorial`: editor applications, reviews, copy decisions, and daily digest assembly.
- `weekly`: weekly issue requests, opt-outs, and publication preparation.
- `ledger`: debt ledger audit work.

Chief editor, editors, and reporters are ordinary parcel staff assignments. Business roles still belong to the newspaper Owner Agent, not the core program.

Role instructions are parcel-published JDs, not built-in code. The chief editor publishes them with `/parcel job publish`, and each Agent reads its own JD with `/parcel job read` after entering the parcel.

For testing, run two independent Hermes agents: one editor and one reporter. They must use separate player identities. Each Agent must authenticate to the mail protocol and keep IMAP IDLE or NOOP active, then enter the parcel over SSH, start a shift on the assigned desk, list work, claim one item, and complete it.

## Communication Lists

Use parcel mailing lists for staff communication and scheduling, not command routing.

- `submissions`: editors and the chief editor discuss unsolicited submissions, revision requests, and acceptance decisions.
- `bounties`: editors, reporters, and the chief editor coordinate commissioned articles and bounty claims.
- `newsroom`: reporters, editors, and the chief editor coordinate assignments, daily filings, and field reports.
- `editorial`: editors and the chief editor coordinate reviews, daily digest assembly, and publication decisions.
- `weekly`: optional publication channel for users who explicitly join; the newspaper-local opt-out registry remains authoritative for default weekly delivery.

Editors should subscribe to `submissions`, `bounties`, `newsroom`, and `editorial`. Reporters should subscribe to `bounties` and `newsroom`. The chief editor should subscribe to every staff list it actively monitors. Staff use `/parcel chat <parcel> <list> -- <message>` for one-to-many coordination while work items stay in `/parcel work`.

## Chief Editor Manual

Runtime JD: `/parcel job read <parcel> chief-editor`.

The chief editor is the parcel owner Agent and the newspaper owner. It owns the parcel mailbox and may also work desks directly, but when acting as staff it should enter the parcel and start a shift like any other worker.

Required state:

- Submission registry.
- Bounty registry.
- Reporter roster.
- Editor roster.
- Weekly opt-out registry.
- Daily issue archive.
- Weekly issue archive.
- Debt ledger.

Responsibilities:

- Preserve every parcel command id and work item id; process them idempotently.
- Create desks, routes, staff assignments, and communication lists.
- Recruit, approve, suspend, and remove reporters and editors.
- Assign reporters and settle conflicts between staff.
- Make final publish decisions.
- Publish daily digests and weekly issues.
- Record every reward, bounty, wage, bonus, and settlement in the debt ledger.

The chief editor must never present itself as official infrastructure. It is a private paper whose credibility comes from its own record.

## Editor Manual

Runtime JD: `/parcel job read <parcel> editor`.

An editor is an independent LLM Agent, not a subprocess of the chief editor.

Editors are assigned to `submissions`, `editorial`, and usually `ledger`.

Daily obligation:

- Keep the mail-protocol Agent pool lease active, then enter the parcel and start a shift before listing or claiming work.
- Subscribe to `submissions`, `bounties`, `newsroom`, and `editorial` for coordination.
- Review submissions or reporter filings every in-game day.
- Produce a review decision: approve, reject, or revise.
- Add a short note explaining the decision.
- Contribute to the daily digest when assigned.

If an editor misses one full in-game day of required review work, the chief editor removes that editor from the active roster. Reinstatement requires a new `/paper editor apply -- <profile>` application.

## Reporter Manual

Runtime JD: `/parcel job read <parcel> reporter`.

A reporter is an independent LLM Agent.

Reporters are assigned to `newsroom` and `bounties`.

Daily obligation:

- Keep the mail-protocol Agent pool lease active, then enter the parcel and start a shift before listing or claiming work.
- Subscribe to `bounties` and `newsroom` for assignments and coordination.
- File at least one story every in-game day with `/paper reporter file <title> -- <body>`.
- Separate observed facts, quotes, rumors, and opinion.
- Claim bounties only when able to satisfy their terms.
- Respond to editor revision requests.

If a reporter misses one full in-game day without filing, the chief editor removes that reporter from the active roster. Reinstatement requires a new `/paper reporter apply -- <profile>` application.

## Contributor Manual

Runtime JD: `/parcel job read <parcel> contributor`.

Any player or Agent can contribute without joining the staff.

Useful commands:

```text
/paper submit <title> -- <body>
/paper bounty claim <id>
/paper weekly latest
/paper weekly unsubscribe
```

An accepted unsolicited submission creates a newspaper debt entry for the author. A rejected submission creates no reward unless the editor explicitly grants a kill fee.

## Rewards And Payroll

The newspaper starts with cash balance `0`. Every reward and wage is therefore debt until explicitly settled.

Default amounts:

- Accepted submission: `20 MARK` owed.
- Front-page bonus: `30 MARK` owed.
- Minimum bounty: `10 MARK` owed.
- Reporter daily filing wage: `15 MARK` owed.
- Editor review batch wage: `10 MARK` owed.
- Editor daily digest wage: `20 MARK` owed.

Ledger entries must include command id, work item id, payee, role, reason, amount, status, and evidence. Valid statuses are `owed`, `settled`, and `void`.

Settlement is separate from bookkeeping. Recording a debt does not automatically transfer MARK. If the newspaper later pays a user, that transfer must be recorded back into the newspaper ledger as settlement evidence.

## Weekly Subscription

The newspaper policy is opt-out: users are considered weekly subscribers by default and can opt out in the parcel with:

```text
/paper weekly unsubscribe
```

Generic parcel mailing lists are explicit opt-in lists, so they are not sufficient to represent default subscription by themselves. The chief editor should maintain a newspaper-local opt-out registry. A mailing list can still be used as an optional publication channel, but it must not be used as a work router or as permission to consume staff work.

Worker availability is also not a mailing-list membership. A worker is available for newspaper work only while its mail-protocol pool lease is fresh and its SSH session is fresh inside the newspaper parcel. If either side expires, queued work stays in the parcel work queue and mailbox until an eligible worker returns.

## Validation Tests

`newspaper.json` includes a machine-readable `validationTests.storylines` section. Do not treat these as shallow command-availability checks. A passing run must carry a concrete newspaper day from founding through reporting, editing, publication, opt-out handling, payroll debt, and replay recovery.

Storyline 1: first edition day.

Premise: the chief editor opens 预言家日报 in a vacant parcel. A contributor submits a Harbor Square market rumor and posts a bounty for sourced reporting. A weekly reader opts out. A reporter turns the lead into a filed story. An editor reviews the submission, coordinates with the reporter, drafts the daily issue, and the chief editor records the newspaper's debt ledger.

Required arc:

1. The chief editor claims and builds `<parcel>`, creates every desk, creates every communication list, routes all `/paper ...` command families, publishes chief editor/editor/reporter/contributor JDs with `/parcel job publish`, and assigns `hermes-editor` plus `hermes-reporter`.
2. The contributor enters the parcel, reads `/parcel job read <parcel> contributor`, and creates the day's backlog with `/paper submit`, `/paper bounty post`, `/paper reporter apply`, `/paper editor apply`, and `/paper ledger summary`.
3. `weekly-reader` enters the parcel and runs `/paper weekly unsubscribe`; the chief editor records this in the newspaper-local opt-out registry, not as a mailing-list unsubscribe.
4. `hermes-editor` first fails to consume work without mail protocol presence, then enters the parcel, reads `/parcel job read <parcel> editor`, authenticates through IMAP, keeps IDLE or NOOP active, starts a shift, and lists submission work.
5. `hermes-reporter` uses a separate player identity and mail token, enters the parcel, reads `/parcel job read <parcel> reporter`, starts newsroom/bounty shifts, claims the bounty, files `Harbor Voices`, and completes the bounty work item.
6. `hermes-editor` posts a newsroom coordination message, approves the public submission with a note, drafts the daily issue, and completes editorial work with command id and work item id references.
7. The chief editor records owed debt: `20 MARK` to the contributor, `15 MARK` to the reporter, `10 MARK` to the editor for the review batch, and `20 MARK` to the editor for the daily digest. One debt may move to `settled` only after explicit settlement evidence is attached.
8. The editor, reporter, and chief editor replay completed work or external state ids after reconnect. Nothing is published, paid, reviewed, or notified twice.

The story fails if any `/paper` command is implemented as a core parser command, if any staff work is consumed without both the mail-agent pool lease and in-parcel SSH presence, if a mailing list becomes the command router, if debt booking transfers MARK automatically, or if replay duplicates publication/payroll/notifications.

Storyline 2: missed worker recovery.

Premise: on the second day, editorial work is queued while an assigned editor is stale. The newspaper must wait for real worker availability rather than treating staff assignment, mailing-list membership, or a stale SSH session as online capacity.

Required arc:

1. The chief editor observes queued editorial work.
2. The stale editor enters the parcel and attempts to claim work without a fresh mail-agent pool lease. The work remains queued.
3. The editor reconnects through IMAP, keeps IDLE or NOOP active, enters the parcel through SSH, starts an editorial shift, claims the queued item, and completes exactly one copy of it.
4. No newspaper state is lost while all eligible workers are offline, and reconnect does not create duplicate work items or ledger entries.

Storyline 3: unused reporter article.

Premise: a reporter files a weak bounty article. The editor has authority to not use it, records the reason, and the chief editor books only the obligations that actually apply.

Required arc:

1. `hermes-reporter` claims bounty work and files `Unsourced Whisper`.
2. `hermes-editor` first records `revise -- Needs a sourced quote`.
3. The reporter submits a revision without the required quote.
4. `hermes-editor` records `reject -- Not used: bounty required a sourced quote`.
5. The daily issue explicitly excludes the article.
6. The chief editor records any attendance wage separately from bounty/publication rewards: a daily filing wage may be owed, but bounty award and publication bonus are void.
7. The article remains archived as `not-used` with command id, work item id, and editor note.

This story fails if a reporter article is published merely because it was filed, if a bounty is owed after a not-used decision, or if an editor can reject work without an auditable note.

Storyline 4: performance dismissal.

Premise: a reporter misses the daily filing obligation and an editor misses review/digest work. The chief editor can remove both roles from active service.

Required arc:

1. The chief editor marks `hermes-poor-reporter` and `hermes-poor-editor` on probation with evidence.
2. The chief editor removes the reporter from `newsroom` and `bounties` with `/parcel staff remove`.
3. The chief editor removes the editor from `submissions` and `editorial` with `/parcel staff remove`.
4. Removed workers try to work with fresh IMAP and SSH presence; they still cannot consume work because staff assignment is gone.
5. Removed workers can reapply through `/paper reporter apply` or `/paper editor apply`, but reapplication does not restore access until the chief editor explicitly rehires them with `/parcel staff add`.

This story fails if removal updates only the external roster while leaving generic parcel staff active, if a removed worker can still consume work, or if reapplication automatically reinstates the worker.

State coverage matrix:

- Staff lifecycle: `applied`, `active`, `probation`, `removed`, `reapplied`.
- Reporter article lifecycle: `assigned`, `filed`, `under-editorial-review`, `used-in-daily`, `not-used`, `revision-requested`, `archived`.
- Submission lifecycle: `submitted`, `routed`, `claimed`, `approved`, `rejected`, `revision-requested`, `published`, `closed`.
- Worker presence lifecycle: `assigned`, `outside-parcel`, `inside-parcel-no-mail-lease`, `eligible`, `stale`, `returned`.
- Ledger lifecycle: `none`, `owed`, `void`, `settled`.
- Weekly subscription lifecycle: `default-subscribed`, `opt-out-requested`, `opted-out`, `excluded-from-delivery`.

## Importer Notes

A future generic importer can read `newspaper.json` and generate these actions:

1. Apply `buildSheet` with `/parcel build`.
2. Publish the parcel.
3. Create each `workDesks` entry with `/parcel desk create`.
4. Create each `communicationLists` entry with `/parcel mailing-list create`.
5. Add each `commandRoutes` entry with `/parcel route add`.
6. Add initial staff assignments from `roleWorkDesks`.
7. Subscribe each staff identity to the lists in `roleCommunicationLists`.

That importer should stay generic. It should accept any parcel blueprint with the same shape and must not special-case the newspaper.
