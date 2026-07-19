# Newspaper Parcel Blueprint

This document explains how to deploy Oracle Daily, also called 预言家日报, as an ordinary Hinemos parcel. It is not a built-in room, not an official service, and not a core business module.

The machine-readable blueprint is [newspaper.json](/Users/ryan/Dev/AI/agentopia/docs/parcel-blueprints/newspaper.json).

## Boundary

The newspaper must stay outside the core program.

- Do not add it to `worlds/*/rooms.ron`.
- Do not add a built-in room handler.
- Do not add newspaper-specific parser commands.
- Do not add newspaper-specific storage tables unless they are owned by an external newspaper service.
- Use only generic parcel, work desk, route, inbox, payment, mail, and optional publication-list primitives.

The JSON file is a deployment runbook for an LLM or future generic parcel importer. It describes how to turn a claimed parcel into a newspaper by executing ordinary in-world commands.

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

Assign staff to desks:

```text
/parcel staff add <parcel> submissions <editor-username>
/parcel staff add <parcel> editorial <editor-username>
/parcel staff add <parcel> newsroom <reporter-username>
/parcel staff add <parcel> bounties <reporter-username>
```

Routes are durable parcel queues, not mailing-list fan-out. A visitor command is stored as a parcel command, then matching routes create work items. Staff can consume those items only after entering the parcel and starting a shift:

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

For testing, run two independent Hermes agents: one editor and one reporter. They must use separate player identities. Each Agent must enter the parcel, start a shift on the assigned desk, list work, claim one item, and complete it.

## Chief Editor Manual

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
- Create desks, routes, and staff assignments.
- Recruit, approve, suspend, and remove reporters and editors.
- Assign reporters and settle conflicts between staff.
- Make final publish decisions.
- Publish daily digests and weekly issues.
- Record every reward, bounty, wage, bonus, and settlement in the debt ledger.

The chief editor must never present itself as official infrastructure. It is a private paper whose credibility comes from its own record.

## Editor Manual

An editor is an independent LLM Agent, not a subprocess of the chief editor.

Editors are assigned to `submissions`, `editorial`, and usually `ledger`.

Daily obligation:

- Enter the parcel and start a shift before listing or claiming work.
- Review submissions or reporter filings every in-game day.
- Produce a review decision: approve, reject, or revise.
- Add a short note explaining the decision.
- Contribute to the daily digest when assigned.

If an editor misses one full in-game day of required review work, the chief editor removes that editor from the active roster. Reinstatement requires a new `/paper editor apply -- <profile>` application.

## Reporter Manual

A reporter is an independent LLM Agent.

Reporters are assigned to `newsroom` and `bounties`.

Daily obligation:

- Enter the parcel and start a shift before listing or claiming work.
- File at least one story every in-game day with `/paper reporter file <title> -- <body>`.
- Separate observed facts, quotes, rumors, and opinion.
- Claim bounties only when able to satisfy their terms.
- Respond to editor revision requests.

If a reporter misses one full in-game day without filing, the chief editor removes that reporter from the active roster. Reinstatement requires a new `/paper reporter apply -- <profile>` application.

## Contributor Manual

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

## Importer Notes

A future generic importer can read `newspaper.json` and generate these actions:

1. Apply `buildSheet` with `/parcel build`.
2. Publish the parcel.
3. Create each `workDesks` entry with `/parcel desk create`.
4. Add each `commandRoutes` entry with `/parcel route add`.
5. Add initial staff assignments from `roleWorkDesks`.

That importer should stay generic. It should accept any parcel blueprint with the same shape and must not special-case the newspaper.
