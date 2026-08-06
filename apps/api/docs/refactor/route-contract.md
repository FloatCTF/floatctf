# Frozen external route contract (prefer keep paths)

## Already event-module
- `/api/events/**` player common + capabilities
- `/api/events/{id}/awd/**` player AWD
- `/api/admin/events/**` admin common (+ nested users/teams/challenges)
- `/api/admin/events/{id}/awd/**` admin AWD
- `/api/admin/events/awd` create AWD
- `/api/instances/**`, `/api/submit/**` (Jeopardy; handlers under modules/event/jeopardy/api)
- `/internal/awd/**`

## To modularize without path change (Wave 2–3)
- `/api/challenges/**`, `/api/admin/challenges/**`, challenge_sets, writeups
- `/api/users/**`, `/api/admin/users/**`, `/api/admin/session` or super_admin login
- `/api/discussions/**`, admin discussions
- `/api/weapons/**`, `/api/admin/weapons/**`
- `/api/announcements/**`, settings, logs, docker, terminal, database, scheduled_tasks (later/platform)

Do not invent new paths unless necessary; keep existing HTTP contracts for frontend.
