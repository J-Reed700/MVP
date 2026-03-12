# Decision Log

Decisions captured from team conversations.
---

### Cron jobs enabled for delegate-bot (cron available to schedu (2026-03-04)

**Decision:** Cron jobs enabled for delegate-bot (cron available to schedule reminders/tasks)
**Reasoning:** Alan confirmed cron jobs are now available. This enables scheduled reminders and background jobs (e.g., recurring reminders, heartbeat-triggered tasks). Logged so we have a record of the capability being turned on.
**Participants:** @Alan Kern, @delegate-bot
**Context:** #mvp — quick runtime update
---

### Enable cron jobs for delegate-bot (cron and reminders availa (2026-03-04)

**Decision:** Enable cron jobs for delegate-bot (cron and reminders available)
**Reasoning:** Alan enabled cron jobs and tested reminders (short 1-minute tests). This enables scheduled recurring jobs, heartbeat-triggered tasks, and user reminders. Suggested next steps were an announcement to #mvp and/or creating a short list of useful cron jobs (daily standup reminders, weekly report, stale-thread nudges).
**Participants:** @Alan Kern, @delegate-bot
**Context:** Heartbeat — 2026-03-04 18:52 (#internal)
---

### Enable cron jobs for delegate-bot (2026-03-04)

**Decision:** Enable cron jobs for delegate-bot
**Reasoning:** Alan enabled cron jobs for the delegate-bot. Heartbeat captured this and noted that cron availability enables scheduled reminders, heartbeat-triggered jobs, stale-thread nudges, exports, and other scheduled behaviors. Logged so we have a permanent record.
**Participants:** Alan Kern, delegate-bot
**Context:** #mvp / heartbeat
---

### Enable cron jobs for the delegate-bot (2026-03-04)

**Decision:** Enable cron jobs for the delegate-bot
**Reasoning:** Cron was enabled (per recent logs). This allows scheduled reminders, heartbeat-triggered jobs, stale-thread nudges, nightly exports, and overdue-ticket alerts. Logged to make the change discoverable and auditable.
**Participants:** @Alan Kern
**Context:** #internal heartbeat
---

### Enable cron jobs for the delegate-bot (2026-03-04)

**Decision:** Enable cron jobs for the delegate-bot
**Reasoning:** Cron jobs were enabled, allowing scheduled reminders, nightly exports, stale-thread nudges, and overdue-ticket alerts. Heartbeat recommended wiring a small starter list (daily async standup reminder + optional Monday sync, nightly tickets export, overdue-ticket alerts).
**Participants:** delegate-heartbeat (system), Alan Kern, Josh
**Context:** #internal — heartbeat digest (19:20)
---

### Enable cron jobs for the delegate‑bot (2026-03-04)

**Decision:** Enable cron jobs for the delegate‑bot
**Reasoning:** Cron jobs were enabled for the delegate-bot (heartbeat observed and recommended next steps). Heartbeat suggested wiring sensible defaults (daily async standup at 10:00 local, nightly tickets export, overdue-ticket alerts) but hit a channel-resolution error when trying to announce to #mvp and needs either permission or the exact channel ID/name.
**Participants:** delegate-heartbeat
**Context:** #internal heartbeat batch
---

### Enable cron jobs for the delegate‑bot (2026-03-04)

**Decision:** Enable cron jobs for the delegate‑bot
**Reasoning:** Cron was enabled for the delegate-bot so it can schedule recurring jobs (daily reminders, nightly exports/backups, stale-thread nudges, overdue-ticket alerts). Heartbeat observed this change and suggested a small starter list of jobs; it attempted to announce but hit a channel-resolution error and is waiting for instruction to proceed.
**Participants:** @Alan Kern, delegate-bot
**Context:** #internal — Heartbeat summary (cron enabled)
---

### Wire defaults for cron jobs: daily async standup @10:00 loca (2026-03-04)

**Decision:** Wire defaults for cron jobs: daily async standup @10:00 local; nightly tickets export/backup; overdue-ticket alerts; optional short Monday morning sync.
**Reasoning:** Cron was enabled and heartbeat recommended starter jobs to reconcile saved standup preferences (Josh prefers async; Alan prefers a morning sync) and to provide useful operational backups/alerts.
**Participants:** delegate-heartbeat (observed), internal team
**Context:** internal
---

### Wire sensible cron defaults: daily async standup @10:00 loca (2026-03-04)

**Decision:** Wire sensible cron defaults: daily async standup @10:00 local; nightly tickets export/backup; overdue-ticket alerts; optional short Monday morning sync to accommodate Alan/Josh preferences.
**Reasoning:** Cron jobs were enabled for the delegate-bot. Standup preferences differ (Josh prefers async; Alan prefers a morning sync). Wiring these defaults reconciles preferences, enables nightly backups/exports, and surfaces overdue tickets for the team.
**Participants:** delegate-heartbeat; team (Alan, Josh)
**Context:** #internal heartbeat batch review
---

### Wire sensible cron defaults: daily async standup @10:00 loca (2026-03-04)

**Decision:** Wire sensible cron defaults: daily async standup @10:00 local; nightly tickets export/backup; overdue-ticket alerts; optional short Monday morning sync.
**Reasoning:** Cron jobs were enabled for the delegate-bot. Standup preferences already saved (Josh = async; Alan = morning sync). The defaults reconcile preferences and make use of the new cron capability; heartbeat proposed these defaults and logged the decision. A minor hiccup prevents posting the starter-job announcement to #mvp because the API couldn't resolve the channel name — heartbeat needs the exact channel ID or posting permission to announce.
**Participants:** delegate-heartbeat (automated), Alan, Josh
**Context:** #internal heartbeat batch (19:49)
---

### Wire sensible cron defaults: daily async standup @10:00 loca (2026-03-04)

**Decision:** Wire sensible cron defaults: daily async standup @10:00 local; nightly tickets export/backup; overdue-ticket alerts; optional short Monday morning sync to reconcile Alan and Josh.
**Reasoning:** Cron jobs were enabled for the delegate-bot and standup preferences were saved (Josh = async; Alan = morning sync). Wiring these defaults uses the newly-available cron capability and reconciles both preferences while adding useful ops (exports and overdue alerts). Heartbeat recommended wiring defaults and attempted to announce but hit a channel-resolution error.
**Participants:** delegate-heartbeat, Alan, Josh
**Context:** #internal (heartbeat batch)
---

### Wire sensible cron defaults — daily async standup @10:00 loc (2026-03-04)

**Decision:** Wire sensible cron defaults — daily async standup @10:00 local; nightly tickets export/backup; overdue-ticket alerts; optional short Monday morning sync to reconcile Alan (morning sync) and Josh (async).
**Reasoning:** Cron jobs were enabled and standup preferences differ between Alan (morning sync) and Josh (async). Wiring sensible defaults puts cron to use immediately (daily async reminder to satisfy Josh, optional Monday short sync to accommodate Alan) while adding nightly exports and overdue-ticket alerts for operational safety.
**Participants:** delegate-heartbeat, Alan, Josh
**Context:** #internal heartbeat batch review — 20:05 tick
---

### Wire sensible cron defaults — daily async standup @10:00 loc (2026-03-04)

**Decision:** Wire sensible cron defaults — daily async standup @10:00 local; nightly tickets export/backup; overdue-ticket alerts; optional short Monday morning sync.
**Reasoning:** Heartbeat detected cron availability and recommended wiring defaults. No cross-channel duplicates, no stale threads >2h, and no blocker language were found. Heartbeat attempted to post a starter-job announcement to #mvp but the API couldn't resolve the channel name; it needs the exact channel ID or post permission. Logged to make the decision permanent.
**Participants:** delegate-heartbeat, Alan, Josh
**Context:** #internal — heartbeat batch review
---

### Wire sensible cron defaults: daily async standup @10:00 loca (2026-03-04)

**Decision:** Wire sensible cron defaults: daily async standup @10:00 local; nightly tickets export/backup; overdue-ticket alerts; optional short Monday morning sync to reconcile Alan (morning sync) and Josh (async).
**Reasoning:** Cron jobs were enabled and standup preferences were already saved (Josh = async; Alan = morning sync). Wiring these defaults gives immediate value (standup reminders, nightly exports, overdue alerts) while offering an optional Monday sync to accommodate both founders. Heartbeat recommended wiring defaults and attempted to post the starter-job announcement but couldn't resolve the #mvp channel name.
**Participants:** delegate-heartbeat, Alan, Josh
**Context:** #internal heartbeat batch review
---

### Upgrade delegate-bot to glm-5 model, replacing gpt-5-mini (2026-03-05)

**Decision:** Upgrade delegate-bot to glm-5 model, replacing gpt-5-mini
**Reasoning:** Alan installed glm-5 as the bot's new brain; immediate positive response from Alan indicates improved performance
**Participants:** Alan Kern
**Context:** #mvp - model upgrade discussion
---

### Enable delegate to read its own source code and architecture (2026-03-05)

**Decision:** Enable delegate to read its own source code and architecture spec (delegate-spec branch)
**Reasoning:** Alan gave delegate additional tooling to read its own spec from the delegate-spec branch, allowing it to understand its own architecture and capabilities for future development
**Participants:** Alan Kern
**Context:** #mvp conversation about giving delegate self-awareness of its own architecture
