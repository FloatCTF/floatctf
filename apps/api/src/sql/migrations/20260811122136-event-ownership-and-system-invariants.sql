-- Event ownership + system-event invariants (forward fix after orthogonalization).
--
-- 1) event_teams composite candidate key (event_id, id)
-- 2) challenge_instances / jeopardy_challenge_solves composite FK → event_teams
-- 3) participant_mode ↔ team_id presence guard on Jeopardy instance/solve rows
-- 4) events.system_key fully immutable on UPDATE (NULL→value also rejected)
-- 5) awd_network_allocations family guard (event must be Awd when event_id set)
-- 6) cosmetic: rename instances_pkey → challenge_instances_pkey when present

-- ── 0. Existing-data validation (abort on invalid rows; do not silently fix) ─

DO $$
DECLARE
    n bigint;
BEGIN
    SELECT count(*) INTO n
    FROM public.challenge_instances ci
    JOIN public.event_teams et ON et.id = ci.team_id
    WHERE ci.team_id IS NOT NULL
      AND et.event_id IS DISTINCT FROM ci.event_id;
    IF n > 0 THEN
        RAISE EXCEPTION
            'event-ownership migration aborted: % challenge_instances rows have cross-event team_id',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.jeopardy_challenge_solves s
    JOIN public.event_teams et ON et.id = s.team_id
    WHERE s.team_id IS NOT NULL
      AND et.event_id IS DISTINCT FROM s.event_id;
    IF n > 0 THEN
        RAISE EXCEPTION
            'event-ownership migration aborted: % jeopardy_challenge_solves rows have cross-event team_id',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.challenge_instances ci
    JOIN public.events e ON e.id = ci.event_id
    WHERE e.participant_mode = 'individual'::public.participant_mode
      AND ci.team_id IS NOT NULL;
    IF n > 0 THEN
        RAISE EXCEPTION
            'event-ownership migration aborted: % individual-mode challenge_instances have team_id set',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.jeopardy_challenge_solves s
    JOIN public.events e ON e.id = s.event_id
    WHERE e.participant_mode = 'individual'::public.participant_mode
      AND s.team_id IS NOT NULL;
    IF n > 0 THEN
        RAISE EXCEPTION
            'event-ownership migration aborted: % individual-mode jeopardy_challenge_solves have team_id set',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.challenge_instances ci
    JOIN public.events e ON e.id = ci.event_id
    WHERE e.participant_mode = 'team'::public.participant_mode
      AND ci.team_id IS NULL;
    IF n > 0 THEN
        RAISE EXCEPTION
            'event-ownership migration aborted: % team-mode challenge_instances have NULL team_id',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.jeopardy_challenge_solves s
    JOIN public.events e ON e.id = s.event_id
    WHERE e.participant_mode = 'team'::public.participant_mode
      AND s.team_id IS NULL;
    IF n > 0 THEN
        RAISE EXCEPTION
            'event-ownership migration aborted: % team-mode jeopardy_challenge_solves have NULL team_id',
            n;
    END IF;

    SELECT count(*) INTO n
    FROM public.awd_network_allocations a
    JOIN public.events e ON e.id = a.event_id
    WHERE e.family IS DISTINCT FROM 'awd'::public.event_family;
    IF n > 0 THEN
        RAISE EXCEPTION
            'event-ownership migration aborted: % awd_network_allocations reference non-Awd events',
            n;
    END IF;
END $$;

-- ── 1. event_teams composite unique (event_id, id) for composite FK target ──

ALTER TABLE public.event_teams
    DROP CONSTRAINT IF EXISTS event_teams_event_id_id_key;

ALTER TABLE public.event_teams
    ADD CONSTRAINT event_teams_event_id_id_key UNIQUE (event_id, id);

COMMENT ON CONSTRAINT event_teams_event_id_id_key ON public.event_teams IS
    'Composite candidate key enabling (event_id, team_id) FKs from Jeopardy ownership tables';

-- ── 2. challenge_instances composite FK (event_id, team_id) ─────────────────
-- Keep event_id → events FK. Replace team_id-only FK with composite ownership FK.
-- MATCH SIMPLE: NULL team_id skips composite check (Individual / Practice).
-- ON DELETE CASCADE matches prior team_id FK semantics (delete team → drop instances).

ALTER TABLE public.challenge_instances
    DROP CONSTRAINT IF EXISTS challenge_instances_team_id_fkey;

ALTER TABLE public.challenge_instances
    DROP CONSTRAINT IF EXISTS challenge_instances_event_team_fkey;

ALTER TABLE public.challenge_instances
    ADD CONSTRAINT challenge_instances_event_team_fkey
    FOREIGN KEY (event_id, team_id)
    REFERENCES public.event_teams (event_id, id)
    ON DELETE CASCADE;

COMMENT ON CONSTRAINT challenge_instances_event_team_fkey ON public.challenge_instances IS
    'Team instances must reference a team belonging to the same event';

-- ── 3. jeopardy_challenge_solves composite FK (event_id, team_id) ────────────
-- ON DELETE CASCADE matches prior team_id FK (delete team → drop solves for that team).
-- Note: deleting a team removes its solves; event-level cascade still deletes via event_id FK.

ALTER TABLE public.jeopardy_challenge_solves
    DROP CONSTRAINT IF EXISTS jeopardy_challenge_solves_team_id_fkey;

ALTER TABLE public.jeopardy_challenge_solves
    DROP CONSTRAINT IF EXISTS jeopardy_challenge_solves_event_team_fkey;

ALTER TABLE public.jeopardy_challenge_solves
    ADD CONSTRAINT jeopardy_challenge_solves_event_team_fkey
    FOREIGN KEY (event_id, team_id)
    REFERENCES public.event_teams (event_id, id)
    ON DELETE CASCADE;

COMMENT ON CONSTRAINT jeopardy_challenge_solves_event_team_fkey ON public.jeopardy_challenge_solves IS
    'Team solves must reference a team belonging to the same event';

-- ── 4. participant_mode ↔ team_id presence guard ────────────────────────────

CREATE OR REPLACE FUNCTION public.assert_jeopardy_participant_ownership()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    pm public.participant_mode;
BEGIN
    IF NEW.event_id IS NULL THEN
        RAISE EXCEPTION 'assert_jeopardy_participant_ownership: event_id is required';
    END IF;

    SELECT e.participant_mode INTO pm
    FROM public.events e
    WHERE e.id = NEW.event_id;

    IF pm IS NULL THEN
        RAISE EXCEPTION
            'assert_jeopardy_participant_ownership: event % not found',
            NEW.event_id;
    END IF;

    IF pm = 'individual'::public.participant_mode AND NEW.team_id IS NOT NULL THEN
        RAISE EXCEPTION
            'assert_jeopardy_participant_ownership: individual event % requires team_id IS NULL',
            NEW.event_id;
    END IF;

    IF pm = 'team'::public.participant_mode AND NEW.team_id IS NULL THEN
        RAISE EXCEPTION
            'assert_jeopardy_participant_ownership: team event % requires team_id IS NOT NULL',
            NEW.event_id;
    END IF;

    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION public.assert_jeopardy_participant_ownership() IS
    'Jeopardy instance/solve rows: individual ⇒ team_id NULL; team ⇒ team_id NOT NULL';

DROP TRIGGER IF EXISTS trg_challenge_instances_participant_ownership
    ON public.challenge_instances;
CREATE TRIGGER trg_challenge_instances_participant_ownership
    BEFORE INSERT OR UPDATE OF event_id, team_id ON public.challenge_instances
    FOR EACH ROW
    EXECUTE FUNCTION public.assert_jeopardy_participant_ownership();

DROP TRIGGER IF EXISTS trg_jeopardy_challenge_solves_participant_ownership
    ON public.jeopardy_challenge_solves;
CREATE TRIGGER trg_jeopardy_challenge_solves_participant_ownership
    BEFORE INSERT OR UPDATE OF event_id, team_id ON public.jeopardy_challenge_solves
    FOR EACH ROW
    EXECUTE FUNCTION public.assert_jeopardy_participant_ownership();

-- ── 5. system_key fully immutable on any UPDATE ─────────────────────────────

CREATE OR REPLACE FUNCTION public.events_identity_immutable()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.family IS DISTINCT FROM NEW.family
       OR OLD.purpose IS DISTINCT FROM NEW.purpose
       OR OLD.participant_mode IS DISTINCT FROM NEW.participant_mode THEN
        RAISE EXCEPTION 'events identity fields (family/purpose/participant_mode) are immutable';
    END IF;
    -- system_key is INSERT-only: reject NULL→value, value→NULL, and value→other.
    IF OLD.system_key IS DISTINCT FROM NEW.system_key THEN
        RAISE EXCEPTION 'events.system_key is immutable';
    END IF;
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION public.events_identity_immutable() IS
    'Blocks UPDATE of family/purpose/participant_mode and system_key (INSERT-only identity)';

-- trigger already exists on events; function body replacement is enough.

-- ── 6. awd_network_allocations family guard (FK stays on events) ────────────

DROP TRIGGER IF EXISTS trg_awd_network_allocations_family ON public.awd_network_allocations;
CREATE TRIGGER trg_awd_network_allocations_family
    BEFORE INSERT OR UPDATE OF event_id ON public.awd_network_allocations
    FOR EACH ROW
    EXECUTE FUNCTION public.assert_event_family('awd');

COMMENT ON TRIGGER trg_awd_network_allocations_family ON public.awd_network_allocations IS
    'Allocations may exist before awd_events configure, but parent event.family must be awd';

-- ── 7. Cosmetic PK rename (optional; ignore if already renamed) ─────────────

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'instances_pkey'
          AND conrelid = 'public.challenge_instances'::regclass
    ) THEN
        ALTER TABLE public.challenge_instances
            RENAME CONSTRAINT instances_pkey TO challenge_instances_pkey;
    END IF;
END $$;
