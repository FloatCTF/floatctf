# File ownership (Wave 2–3 parallel agents)

| Agent | Exclusive paths | Must NOT touch |
|-------|-----------------|----------------|
| Challenge | `src/modules/challenge/**`, migrate from `api/{admin,service}/challenge*.rs` | bootstrap/state, lib.rs |
| Identity | `src/modules/identity/**`, `core/security/**`, `api/extractor/**` (new), migrate auth/users/super_admin | bootstrap/state (report patch only) |
| Community | `src/modules/community/**`, migrate discussions handlers | bootstrap/state |
| Weapon | `src/modules/weapon/**`, migrate weapons handlers | bootstrap/state |
| OOB | **no module** — entity-only dead code | — |
| Coordinator | bootstrap, modules/mod.rs, api/mod.rs, route aggregation | after wave agents done |

Each agent exposes:
- `configure_player_routes` / `configure_admin_routes` as needed
- `*Services` struct if useful
- deletes old handlers when done
