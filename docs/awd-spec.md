# FloatCTF AWD Final Specification v1

Status: FROZEN PRODUCT SPEC
Scope: AWD Competition Mode
Purpose: This document is the authoritative product specification for FloatCTF AWD.

Existing implementation, historical migrations, comments, tests, and older
documentation MUST NOT override the semantics defined here.

Implementation details such as exact database enum names may follow the existing
architecture where reasonable, but observable AWD behavior MUST conform to this
specification.

---

# 1. Core Model

AWD is a continuous attack-and-defense competition.

Each team owns one or more GameBoxes.

After the competition starts:

- teams may continuously repair and modify their own GameBoxes;
- during Attack, teams may attack other teams;
- services are checked once per round;
- dynamic Flags rotate by round;
- successful attacks transfer score from victim to attacker;
- service failures deduct score;
- GameBoxes are mutable and are not restored automatically by the platform.

The competition has one optional Hardening stage followed by the Attack stage.

Hardening occurs at most once per Event.

It MUST NOT occur again between Attack rounds.

---

# 2. Competition Time Model

The administrator configures:

- `round_count`
- `round_duration`

`event_duration` is derived from the generic Event schedule:

```text
event_duration = events.end_time - events.start_time
```

This is NOT persisted as a duplicate AWD column.

Attack duration is derived:

```text
attack_duration = round_count × round_duration
```

Hardening duration is derived:

```text
hardening_duration = event_duration - attack_duration
```

Configuration MUST satisfy:

```text
round_count IS NOT NULL
round_count > 0
round_duration > 0
events.end_time must exist
events.end_time > events.start_time
event_duration >= round_count × round_duration
```

If:

```text
round_count × round_duration > event_duration
```

the configuration is invalid and MUST be rejected.

The platform MUST NOT:

* shorten rounds automatically;
* reduce the configured round count;
* create a negative Hardening duration.

## 2.1 Hardening = 0

`hardening_duration = 0` is explicitly supported.

In this case:

```text
Start Event
→ immediately enter Attack
→ immediately start Round 1
```

There is no Hardening stage.

This means teams may attack and repair from the beginning of the competition.

## 2.2 Pause and configured duration

Paused time does NOT consume:

* Hardening time;
* current Round time;
* configured Event competition time.

Final Judge settlement after the last round is also outside the configured
`event_duration`.

Therefore wall-clock time may exceed `event_duration` because of:

* Pause;
* final Judge settlement.

---

# 3. Lifecycle

Conceptual lifecycle:

```text
Draft / Configuration
        ↓
Deploy
        ↓
Deployed
        ↓
Precheck
        ↓
Verified
        ↓
Start
        ↓
Hardening
(if duration > 0)
        ↓
Attack
  ├─ Round 1
  ├─ Round 2
  ├─ ...
  └─ Round N
        ↓
Final Settlement
        ↓
Finished
        ↓
Archived
```

The exact persistence representation may continue using the project's existing
Event status + AWD phase architecture if appropriate.

Do NOT introduce new persisted states merely to match the names in this diagram
unless the implementation actually requires them.

---

# 4. Precheck

Precheck is a hard gate.

Normal Event Start MUST only be possible after Precheck has completely passed.

Expected flow:

```text
Deployed
→ Prechecking
→ Verified
→ Start
```

If verification fails:

```text
Prechecking
→ VerificationFailed
```

the Event MUST NOT start.

There is no normal Start bypass.

Do NOT implement an implicit "force start despite failed Precheck".

Precheck should validate at minimum the existing AWD infrastructure categories,
including:

* configuration validity;
* container/runtime availability;
* expected GameBox instances;
* WireGuard;
* network/firewall matrix;
* Flag infrastructure;
* Judge infrastructure.

---

# 5. Hardening

Hardening occurs once before Attack.

It MUST NOT occur once per Round.

Hardening duration is derived from the time model.

Administrators MUST NOT manually finish Hardening early.

Transition to Attack occurs automatically when the complete Hardening duration
has elapsed.

During Hardening:

* teams may access their own GameBoxes;
* teams may SSH into their own GameBoxes;
* teams may modify and repair their own GameBoxes;
* Reset is allowed;
* teams cannot access other teams' GameBoxes;
* GameBoxes cannot access other teams' GameBoxes;
* Judge does not run;
* no Judge score is produced;
* attack Flags are not active;
* Flag submissions are not accepted;
* attack scoring is disabled.

When Hardening expires:

```text
Hardening
→ Attack
→ Round 1
```

---

# 6. Attack

During Attack:

* teams may SSH into their own GameBoxes continuously;
* teams may repair their own services continuously;
* players may attack other teams' GameBoxes;
* GameBoxes may attack other teams' GameBoxes;
* dynamic Flags are active;
* Flag submissions are active;
* Judge is active;
* scoring is active.

Entering Attack does NOT freeze GameBox modification.

AWD is explicitly a continuous:

```text
attack + defend + repair
```

competition.

---

# 7. Rounds

Attack consists of exactly:

```text
round_count
```

Rounds.

Each Round has exactly:

```text
round_duration
```

of active competition time.

Rounds transition automatically.

Administrators MUST NOT manually finish the current Round early.

If the competition must stop temporarily, the administrator uses Pause instead.

Conceptually:

```text
Round 1
→ Round 2
→ Round 3
→ ...
→ Final Round
```

There is no Hardening transition between Rounds.

---

# 8. Dynamic Flag Model

Flags are scoped to the active Round and target GameBox instance.

Existing deterministic dynamic Flag semantics may be retained.

At minimum a Flag must be cryptographically bound to:

* Event;
* Round;
* target GameBox instance.

Plaintext Flags SHOULD NOT need to be persisted when deterministic derivation is
used.

Flag validation must ensure that the submitted Flag belongs to the currently
active Round.

## 8.1 Round expiration

When Round N ends:

```text
all Round N Flags immediately become invalid
```

There is NO previous-round Flag grace period.

When Round N+1 begins, only Round N+1 Flags may be submitted.

---

# 9. Attack Submission

A valid attack submission must satisfy at least:

* Event is in Attack;
* attacker is an eligible team;
* attacker is not banned;
* target team is not banned;
* submitted Flag is valid;
* submitted Flag belongs to the current Round;
* attacker is not the victim.

A team may score at most once against the same EventGameBox in one Round.

Uniqueness semantics:

```text
(attacker_team, round, event_gamebox)
```

Example:

```text
Team A → Team B / web-1 in Round 5
first valid submission  → scores
later duplicate         → does not score
```

Different attacking teams MAY independently score against the same target in the
same Round.

Example:

```text
Team B / web-1

Team A → success
Team C → success
Team D → success
```

All three may score once.

---

# 10. Attack Scoring

Score weights are configurable per EventGameBox.

Each EventGameBox has:

```text
attack_score
```

Successful attack scoring is symmetric:

```text
Attacker +attack_score
Victim   -attack_score
```

The same EventGameBox `attack_score` magnitude is used for both sides.

This does NOT require every EventGameBox in the Event to have the same
`attack_score`.

There is no separate victim-loss amount.

The victim loses exactly the same amount the attacker gains.

Total team score MAY become negative.

There MUST NOT be a lower bound of zero.

---

# 11. Initial Score

Each Team starts the Event with a configurable initial score:

```text
initial_score
```

This is the score baseline before attack, Judge, Reset, and other scoring events.

The initial score may be materialized as a ledger event or represented using an
equivalent auditable mechanism consistent with the existing score architecture.

---

# 12. First Blood

First Blood exists.

Scope:

```text
Event × EventGameBox
```

There is exactly ONE First Blood bonus for each EventGameBox during the entire
competition.

It is NOT reset every Round.

Example:

```text
EventGameBox: web-1

Round 1:
Team A is first successful attacker
→ Team A gets First Blood

Round 2+:
no team can receive First Blood for web-1 again
```

The First Blood attacker receives:

```text
attack_score + first_blood_bonus
```

The victim still loses only:

```text
attack_score
```

First Blood therefore creates an additional attacker bonus and is not an
additional victim penalty.

Race conditions MUST be handled transactionally/idempotently so only one team
can receive the First Blood bonus.

---

# 13. Judge Timing

Judge checks each GameBox exactly once per Round.

The Judge result belongs to the Round that has just ended.

There is NO baseline Judge when Attack starts.

Example:

```text
Attack Start
→ Round 1
→ Round 1 End
    ├─ create Judge tasks for Round 1
    └─ start Round 2 immediately
```

Judge therefore evaluates the result of the previous Round.

---

# 14. Judge and Round Independence

Round progression MUST NOT wait for Judge completion.

At a normal Round boundary:

```text
Round N ends
    ├─ Round N Flags expire
    ├─ create Round N Judge tasks
    └─ immediately start Round N+1
```

Judge tasks execute asynchronously.

The Round clock remains authoritative and independent of Judge execution time.

A slow, retrying, or temporarily unavailable Judge worker MUST NOT delay the
next Round.

---

# 15. No Judge Snapshot

The platform does NOT freeze or snapshot GameBoxes at a Round boundary.

Example:

```text
12:05:00 Round 1 ends
12:05:00 Round 2 starts
12:05:02 team modifies service
12:05:04 Judge checks service
```

The live service state observed by Judge at execution time is accepted as the
Round 1 Judge result.

This approximation is intentional.

The platform MUST NOT pause SSH, snapshot containers, or freeze filesystems just
to obtain an exact Round-boundary state.

---

# 16. Judge Worker Architecture

AWD Judge must use:

```text
Pull + Lease + Heartbeat
```

AWD retains its own domain queue:

```text
awd_judge_tasks
```

AWD MUST NOT reuse AWDP domain tables such as `awdp_evaluations`.

Generic worker claim/lease infrastructure may be shared where architecture
supports it.

Conceptually:

```text
API / DB
  ↓
Pending AWD Judge Tasks
  ↓
JudgeServer claims work
  ↓
Lease
  ↓
Heartbeat while running
  ↓
Result
```

A lease prevents abandoned Running tasks from becoming permanently stuck.

An expired lease may be reclaimed according to retry policy.

A stale worker result from an obsolete lease/attempt MUST NOT overwrite the
currently valid attempt.

---

# 17. Judge Outcomes

Judge must distinguish GameBox/service failure from platform/Judge
infrastructure failure.

## 17.1 Up

```text
Up
→ no score change
```

Healthy services do NOT earn additional points.

## 17.2 Down

If Judge successfully executes the service check and determines that the
GameBox/service is unavailable, including a target service timeout:

```text
Down
→ -judge_down_penalty
```

The penalty is independent of downtime duration.

One GameBox that is Down at its single Round check produces one penalty.

Example:

```text
Team A has four GameBoxes:

web-1   Up
web-2   Down
api-1   Down
pwn-1   Up

→ 2 × judge_down_penalty
```

There is no per-second downtime scoring.

## 17.3 JudgeError

Failures caused by Judge/platform infrastructure are NOT team failures.

Examples:

* JudgeServer worker crash;
* internal API failure;
* Judge platform execution failure;
* worker infrastructure failure.

These should be retried.

After the configured maximum retry policy is exhausted:

```text
JudgeError
```

is terminal and does NOT deduct team score.

JudgeError MUST NOT leave final settlement blocked forever.

## 17.4 Resetting

If Judge encounters a GameBox while the platform is actively rebuilding it due
to an explicit Reset:

```text
SkippedResetting
```

No JudgeDown penalty is applied for that task.

---

# 18. Final Round and Final Settlement

The final Round behaves differently only after its timer expires.

When the final Round ends:

```text
Final Round Ends
    ├─ final-round Flags expire
    ├─ stop accepting attack submissions
    ├─ create final-round Judge tasks
    └─ enter Final Settlement
```

There is no next Round.

The Event MUST NOT transition to Finished until all final-round Judge tasks have
reached acceptable terminal states and final scoring is settled.

Terminal outcomes include appropriate final states such as:

* Up;
* Down;
* JudgeError;
* other explicitly defined non-retryable terminal outcomes.

The Event must never remain permanently stuck because a Judge worker disappeared.

Only after final settlement completes:

```text
Final Settlement
→ Finished
```

Therefore:

```text
Finished = final scoreboard is stable
```

---

# 19. Reset

Players may Reset their own GameBoxes during the active competition.

Reset is allowed during:

* Hardening;
* Attack.

Reset is NOT allowed while:

* Event is Paused;
* Team is Banned;
* Event is Finished.

## 19.1 Reset semantics

Reset is destructive.

```text
Reset
→ destroy old physical container
→ recreate from official initial image
```

All team modifications in the old container are lost.

The logical GameBox identity remains stable where applicable, including:

* Event/Team/GameBox mapping;
* logical instance identity;
* assigned internal IP;
* access identity/credentials according to existing architecture.

Reset is not equivalent to `docker restart`.

## 19.2 Reset quota

Reset supports:

```text
free_resets
reset_penalty
```

The first configured number of Resets are free.

After the quota is exhausted, every additional Reset creates a score penalty:

```text
-reset_penalty
```

The total team score may become negative.

## 19.3 No Reset protection

There is NO post-Reset protection window.

After Reset completes:

```text
GameBox immediately returns to normal competition rules
```

During Attack this means:

* own team can immediately access it;
* other teams can immediately attack it;
* Flags work normally;
* future Judge checks work normally.

The platform MUST NOT provide temporary immunity after Reset.

---

# 20. GameBox Failure Responsibility

The platform MUST NOT automatically restart or recreate an individual GameBox
merely because its container stopped or exited.

If a team breaks/stops its own GameBox:

```text
GameBox becomes unavailable
→ team is responsible
→ Judge may judge service Down
→ team may choose explicit Reset
```

The platform MUST NOT silently perform a pristine rebuild.

A pristine rebuild is an explicit Reset operation and therefore follows Reset
quota/penalty semantics.

Individual GameBox failures are not grounds for pausing the whole Event.

---

# 21. Platform Infrastructure Failure

Platform infrastructure failure is different from GameBox failure.

Examples include failures in core infrastructure such as:

* Docker daemon / runtime host;
* competition host;
* core AWD network infrastructure;
* WireGuard infrastructure;
* equivalent platform-wide components that compromise competition fairness.

A confirmed platform-level failure that compromises the competition MUST
automatically Pause the Event.

The system MUST NOT infer platform failure merely because one or a few GameBoxes
are unavailable.

This is required to prevent teams from intentionally stopping GameBoxes to
trigger a global Pause.

After platform recovery:

```text
administrator manually Resume
```

The platform MUST NOT automatically Resume the competition.

---

# 22. Pause

Pause means complete player-side competition freeze.

Containers remain running.

While Paused:

* players cannot SSH into their own GameBoxes;
* players cannot access other GameBoxes;
* players cannot submit Flags;
* players cannot obtain attack Flags;
* players cannot Reset;
* Hardening timer is frozen if currently in Hardening;
* Round timer is frozen if currently in Attack;
* competition time is frozen.

In-flight Judge tasks may finish and their technical results may be recorded.

However Judge completion while the Event is Paused MUST NOT produce competition
score.

Resume restores the Event to the phase that was active before Pause and resumes
the remaining phase/Round time.

---

# 23. Ban

Ban has no timer.

There is NO automatic Ban expiration.

A Team remains Banned until an administrator explicitly performs Unban.

Ban means complete suspension of that Team's competition behavior.

While Banned, the Team:

* cannot SSH into its GameBoxes;
* cannot access other teams' GameBoxes;
* cannot submit Flags;
* cannot obtain attack Flags;
* cannot Reset;
* cannot generate new attack score;
* does not participate in normal Judge scoring.

The Team's GameBox containers remain running.

Ban MUST NOT:

* stop GameBox containers;
* destroy GameBoxes;
* Reset GameBoxes.

Historical score earned before Ban is preserved.

Ban does NOT zero or roll back existing score.

## 23.1 Attacking banned teams

A Banned Team is removed from the active attack surface.

Other teams MUST NOT be able to continue scoring against the Banned Team.

Therefore while Team B is Banned:

* other players cannot attack Team B GameBoxes through the competition network;
* attack Flags for Team B are not eligible for scoring;
* Team B cannot be farmed for score;
* Team B Judge scoring is suspended.

After explicit Unban, the Team returns to the rules of the current Event phase.

---

# 24. Finished

Once the Event reaches Finished:

* all player GameBox access is disabled;
* SSH is disabled;
* attacks are disabled;
* Flag submission is disabled;
* Reset is disabled;
* scoring is final.

GameBox containers may remain available to administrators temporarily for:

* export;
* archival;
* review;
* incident analysis;
* post-event operations.

Players cannot modify the environment after Finished.

---

# 25. Network Isolation

All GameBox Docker networks MUST be isolated from the public Internet.

GameBoxes MUST NOT have public Internet access.

This must be enforced at the network/firewall level rather than relying only on
application behavior inside individual containers.

At minimum:

```text
GameBox → Internet = DENY
GameBox → Host     = DENY
```

Only explicitly required internal competition services may be reachable.

---

# 26. Hardening Network Matrix

During Hardening:

```text
Player → own Team GameBox       ALLOW
Player → other Team GameBox     DENY

GameBox → same Team GameBox     ALLOW
GameBox → other Team GameBox    DENY

GameBox → Internet              DENY
GameBox → Host                  DENY

Player → protected infra        DENY
```

Necessary competition-internal routes may be explicitly whitelisted.

Hardening isolation MUST apply to both:

* direct player traffic;
* GameBox-originated traffic.

A player MUST NOT be able to bypass Hardening by SSHing into their own GameBox
and attacking another Team from there.

---

# 27. Attack Network Matrix

During Attack:

```text
Player → own Team GameBox       ALLOW
Player → other Team GameBox     ALLOW

GameBox → same Team GameBox     ALLOW
GameBox → other Team GameBox    ALLOW

GameBox → Internet              DENY
GameBox → Host                  DENY
```

Cross-Team GameBox traffic is intentionally legal during Attack.

This permits legitimate attack chains such as:

```text
Player
→ compromise GameBox A
→ pivot from GameBox A
→ attack GameBox B
```

The platform MUST NOT force all attacks to originate directly from the player's
WireGuard endpoint.

---

# 28. Competition Infrastructure Traffic

Infrastructure services may have privileged network paths required for operation.

Examples:

```text
JudgeServer → GameBox     ALLOW as required for service checks
GameBox → FlagServer      ALLOW as required by dynamic Flag design
```

Player access to protected platform infrastructure should remain denied unless
there is an explicit product requirement.

Infrastructure routes should be whitelist-based.

They MUST NOT accidentally provide GameBoxes with a route to:

* the public Internet;
* host management services;
* Docker socket;
* unrelated platform control-plane services.

---

# 29. Score Ledger Requirements

Score changes should remain auditable and idempotent.

Required logical score events include at least:

* InitialScore;
* Attack;
* FirstBlood;
* JudgeDown;
* ResetPenalty;
* administrator Adjustment if the platform supports it.

`Judge Up` does not need to create a score delta.

Attack scoring must atomically produce the corresponding attacker and victim
effects.

Duplicate submissions, Judge retries, worker retries, and callback/result
replays MUST NOT duplicate score.

Scores may be negative.

---

# 30. Reliability Invariants

The following invariants are mandatory:

1. A failed Precheck cannot normally start the Event.
2. Hardening happens at most once.
3. Hardening cannot be ended manually before its configured duration.
4. Round duration is fixed.
5. Round progression does not wait for Judge.
6. Old-Round Flags immediately expire.
7. Judge retries cannot duplicate Judge penalties.
8. Attack retries/submission replays cannot duplicate attack score.
9. First Blood can only be awarded once per EventGameBox per Event.
10. Reset cannot silently become an automatic platform recovery operation.
11. Individual GameBox failure cannot Pause the Event.
12. Platform infrastructure failure may automatically Pause the Event.
13. Automatic Pause is never followed by automatic Resume.
14. Banned teams cannot participate in either side of attack scoring.
15. Banning a team does not stop its containers.
16. Finished means the final scoreboard is stable.
17. GameBoxes never have public Internet connectivity.
18. GameBox-to-GameBox cross-Team traffic is denied during Hardening and allowed
    during Attack.
19. A Judge/platform failure cannot incorrectly penalize a team.
20. A permanently lost Judge worker cannot leave final settlement stuck forever.

---

# 31. Explicit Non-Goals

The AWD implementation MUST NOT introduce the following unless this
specification is deliberately revised:

* per-Round Hardening;
* previous-Round Flag grace period;
* baseline Judge at Attack start;
* synchronous Round progression waiting on Judge;
* exact GameBox snapshot at Round boundary;
* Judge UP bonus score;
* timed Ban / automatic Unban;
* Ban-triggered GameBox shutdown;
* post-Reset immunity/protection;
* automatic individual GameBox pristine recovery;
* GameBox public Internet access;
* restriction requiring attacks to originate only from player WireGuard;
* AWDP evaluation tables as AWD Judge domain storage.

---

# 32. Source of Truth

When implementation behavior conflicts with this document:

```text
this specification wins
```

Before changing implementation, engineers should identify the exact difference
between:

```text
CURRENT SOURCE REALITY
vs.
TARGET AWD SPEC
```

and implement migrations/refactors intentionally rather than layering permanent
compatibility behavior over obsolete AWD semantics.
