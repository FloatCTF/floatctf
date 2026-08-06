# FloatCTF API Documentation

## Project Structure

```
floatctf-api/
├── Cargo.toml
├── src/
│   ├── main.rs                    # Application entry point
│   ├── api/
│   │   ├── mod.rs                 # API module exports
│   │   ├── admin/                 # Admin routes (SuperAdmin JWT required)
│   │   │   ├── mod.rs             # Admin route configuration
│   │   │   ├── dto.rs             # Shared DTOs
│   │   │   ├── announcements.rs
│   │   │   ├── challenge_sets.rs
│   │   │   ├── challenges.rs
│   │   │   ├── database.rs
│   │   │   ├── docker.rs
│   │   │   ├── event_announcements.rs
│   │   │   ├── event_challenges.rs
│   │   │   ├── event_logs.rs
│   │   │   ├── event_teams.rs
│   │   │   ├── event_users.rs
│   │   │   ├── event_writeups.rs
│   │   │   ├── events.rs
│   │   │   ├── instances.rs
│   │   │   ├── logs.rs
│   │   │   ├── scheduled_tasks.rs
│   │   │   ├── settings.rs
│   │   │   ├── super_admin.rs
│   │   │   ├── system.rs
│   │   │   ├── users.rs
│   │   │   └── weapons.rs
│   │   └── service/               # Service routes (User JWT required)
│   │       ├── mod.rs             # Service route configuration
│   │       ├── announcements.rs
│   │       ├── challenge_sets.rs
│   │       ├── challenge_solves.rs
│   │       ├── challenge_writeups.rs
│   │       ├── challenges.rs
│   │       ├── events.rs
│   │       ├── instances.rs
│   │       ├── submit.rs
│   │       ├── super_admin.rs
│   │       ├── uploads.rs
│   │       ├── users.rs
│   │       └── weapons.rs
│   ├── auth.rs                    # JWT authentication (HS512)
│   ├── config.rs                  # Configuration management
│   ├── db.rs                      # Database initialization
│   ├── entity/                    # SeaORM entity definitions
│   │   ├── challenges.rs
│   │   ├── events.rs
│   │   ├── instances.rs
│   │   ├── users.rs
│   │   └── ...
│   └── strategies/                 # Event strategies
│       └── event/
│           ├── implementations/
│           │   ├── jeopardy_practice.rs
│           │   ├── jeopardy_single.rs
│           │   └── jeopardy_team.rs
│           └── trait_def.rs
```

## Technology Stack

- **Web Framework**: Actix-web 4
- **ORM**: Sea-ORM 1.1 (PostgreSQL)
- **Authentication**: JWT (HS512 algorithm, 8-hour default expiration)
- **Docker Management**: Bollard 0.19
- **Password Hashing**: Argon2

## Authentication

### JWT Token Structure

Both User and SuperAdmin JWTs contain:
- `sub`: User ID (UUID)
- `role`: Role enum (`User`, `SuperAdmin`, `ResetAccount`, `AwdJudger`)
- `exp`: Expiration timestamp

### Headers

```
Authorization: Bearer <token>
```

### Roles

| Role | Description |
|------|-------------|
| `User` | Regular user access |
| `SuperAdmin` | Admin panel access |
| `ResetAccount` | Password reset token |
| `AwdJudger` | Award judging access |

---

## Public Routes (No Authentication)

### Authentication

#### `POST /api/admin/session` - Super Admin Login

Super admin authentication.

**Request Body:**
```json
{
  "username": "string",
  "password": "string"
}
```

**Response:** `UniResponse<String>` - JWT token

---

#### `POST /api/users/session` - User Login

User authentication.

**Request Body:**
```json
{
  "username": "string",
  "password": "string"
}
```

**Response:** `UniResponse<String>` - JWT token

---

#### `POST /api/users` - User Registration

Create a new user account.

**Request Body:**
```json
{
  "username": "string",
  "nickname": "string",
  "password": "string",
  "email": "string"
}
```

**Response:** `UniResponse<String>` - Success message

---

#### `POST /api/users/reset_password` - Send Password Reset Email

Send password reset link via email.

**Request Body:**
```json
{
  "email": "string"     // optional
  "username": "string"  // optional (one required)
}
```

**Response:** `UniResponse<()>`

---

#### `POST /api/users/reset` - Reset Password with Token

Reset password using the token from email.

**Query Parameters:**
- `token`: Password reset token

**Request Body:**
```json
{
  "password": "string",
  "confirmed_password": "string"
}
```

**Response:** `UniResponse<()>`

---

## Service Routes (`/api/*`) - Requires User JWT

### Users

#### `GET /api/users/me` - Get Current User Profile

**Response:** `UniResponse<users::Model>`
```json
{
  "id": "uuid",
  "username": "string",
  "nickname": "string",
  "email": "string",
  "password": "string",
  "created_at": "datetime",
  "updated_at": "datetime"
}
```

---

#### `PATCH /api/users/me` - Update Current User Profile

**Request Body:**
```json
{
  "nickname": "string",   // optional
  "email": "string",     // optional
  "password": "string"   // optional
}
```

**Response:** `UniResponse<()>`

---

### Announcements

#### `GET /api/announcements` - List Announcements

**Response:** `UniResponse<Vec<announcements::Model>>`

---

### Weapons

#### `GET /api/weapons` - List Weapons

**Response:** `UniResponse<Vec<weapons::Model>>`

---

### Challenges

#### `GET /api/challenges` - List Visible Challenges

**Query Parameters:**
- `page`: Page number (optional)
- `limit`: Items per page (optional)
- `filter`: JSON filter expression (optional)
  - `id`: Filter by challenge ID
  - `name`: Filter by name (contains)
  - `category`: Filter by category (contains)
  - `description`: Filter by description (contains)

**Response:** `UniResponse<Vec<challenges::Model>>` with meta
```json
{
  "data": [
    {
      "id": "uuid",
      "name": "string",
      "safe_name": "string",
      "category": "string",
      "description": "string",
      "attachment": "string|null",
      "hidden": false,
      "toml_str": "string",
      "created_at": "datetime",
      "updated_at": "datetime"
    }
  ],
  "meta": {
    "page": 1,
    "limit": 10,
    "total": 100
  }
}
```

---

#### `GET /api/challenges/{challenge_id}` - Get Challenge Details

**Path Parameters:**
- `challenge_id`: Challenge UUID

**Response:** `UniResponse<challenges::Model>`

---

#### `GET /api/challenges/{challenge_id}/instance` - Get Challenge Instance

Get user's practice instance for a challenge.

**Path Parameters:**
- `challenge_id`: Challenge UUID

**Response:** `UniResponse<instances::Model>`

---

#### `POST /api/challenges/{challenge_id}/my_writeup` - Submit Challenge Writeup

**Path Parameters:**
- `challenge_id`: Challenge UUID

**Request Body:**
```json
{
  "content": "string"
}
```

**Response:** `UniResponse<challenge_writeup::Model>`

---

#### `GET /api/challenges/{challenge_id}/my_writeup` - Get User's Writeup

**Path Parameters:**
- `challenge_id`: Challenge UUID

**Response:** `UniResponse<challenge_writeup::Model>`

---

#### `GET /api/challenges/{challenge_id}/writeups` - List Challenge Writeups

**Path Parameters:**
- `challenge_id`: Challenge UUID

**Response:** `UniResponse<Vec<ChallengeWriteupResult>>`
```json
{
  "data": [
    {
      "nickname": "string",
      "email": "string",
      "challenge": {...},
      "writeup": {...}
    }
  ]
}
```

---

### Challenge Sets

#### `GET /api/challenge_sets` - List Challenge Sets

**Response:** `UniResponse<Vec<challenge_sets::Model>>`

---

#### `GET /api/challenge_sets/{challenge_set_id}` - Get Challenge Set

**Path Parameters:**
- `challenge_set_id`: Challenge Set UUID

**Response:** `UniResponse<challenge_sets::Model>`

---

### Instances

#### `GET /api/instances` - List User's Instances

**Query Parameters:**
- `page`: Page number (optional)
- `limit`: Items per page (optional)
- `filter`: JSON filter expression (optional)
  - `id`: Filter by instance ID
  - `status`: Filter by status (`Running`, `Stopped`, etc.)
  - `ref`: Filter by reference (contains)
  - `challenge_id`: Filter by challenge ID
  - `gamebox_id`: Filter by gamebox ID

**Response:** `UniResponse<Vec<instances::Model>>` with meta

---

#### `GET /api/instances/{instance_id}` - Get Instance Details

**Path Parameters:**
- `instance_id`: Instance UUID

**Response:** `UniResponse<instances::Model>`

---

#### `POST /api/instances/launch` - Launch New Instance

**Request Body:**
```json
{
  "event_id": "uuid|null",   // optional, for event challenges
  "challenge_id": "uuid"
}
```

**Response:** `UniResponse<instances::Model>`

---

#### `DELETE /api/instances/{instance_id}` - Destroy Instance

**Path Parameters:**
- `instance_id`: Instance UUID

**Response:** `UniResponse<()>`

---

### Solves

#### `GET /api/challenge_solves` - List User's Solves

**Response:** `UniResponse<Vec<challenge_solves::Model>>`

---

#### `GET /api/challenge_solves/top15users` - Get Top 15 Users

**Response:** `UniResponse<Vec<...>>`

---

### Events

#### `GET /api/events` - List Visible Events

**Query Parameters:**
- `page`: Page number (optional)
- `limit`: Items per page (optional)
- `filter`: JSON filter expression (optional)
  - `id`: Filter by event ID
  - `title`: Filter by title (contains)
  - `type`: Filter by event type (`JeopardyPractice`, `JeopardySingle`, `JeopardyTeam`)
  - `allow_join`: Filter by allow_join boolean

**Response:** `UniResponse<Vec<EventInfo>>`
```json
{
  "data": [
    {
      "event": {...},
      "team_result": {...} | null,
      "joined": true | false
    }
  ]
}
```

---

#### `GET /api/events/{event_id}` - Get Event Details

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<EventInfo>`

---

#### `GET /api/events/{event_id}/challenges` - Get Event Challenges

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<EventChallengeResult>>`
```json
{
  "data": [
    {
      "challenge": {...},
      "current_points": 500.0,
      "solved_count": 10,
      "solved": true | false,
      "solved_no": 3
    }
  ]
}
```

---

#### `GET /api/events/{event_id}/instances` - Get Event Instances

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<EventInstanceResult>>`

---

#### `GET /api/events/{event_id}/challenges/{challenge_id}/instance` - Get Event Challenge Instance

**Path Parameters:**
- `event_id`: Event UUID
- `challenge_id`: Challenge UUID

**Response:** `UniResponse<instances::Model>`

---

#### `GET /api/events/{event_id}/scoreboard` - Get Scoreboard

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<ScoreboardItem>>`
```json
{
  "data": [
    {
      "no": 1,
      "name": "string",
      "score": 1000.0,
      "solved_count": 5,
      "challenges": [
        {
          "name": "string",
          "solved": true,
          "solved_no": 2
        }
      ]
    }
  ]
}
```

---

#### `GET /api/events/{event_id}/announcements` - Get Event Announcements

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<event_announcements::Model>>`

---

#### `GET /api/events/{event_id}/trend` - Get Trend Data

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<TrendItem>>`

---

#### `GET /api/events/{event_id}/submit_wp_status` - Get Writeup Status

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<DateTime>` - Last writeup submission time

---

#### `POST /api/events/{event_id}/join` - Join Event

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<event_users::Model>`

---

#### `DELETE /api/events/{event_id}/leave` - Leave Event

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<u64>` - Deleted count

---

#### `POST /api/events/{event_id}/team` - Create Team

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "name": "string"
}
```

**Response:** `UniResponse<event_teams::Model>`

---

#### `POST /api/events/{event_id}/team/{team_id}/join` - Join Team

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Response:** `UniResponse<()>`

---

#### `POST /api/events/{event_id}/team/{team_id}/leave` - Leave Team

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Response:** `UniResponse<()>` - Error if captain

---

#### `DELETE /api/events/{event_id}/team/{team_id}` - Quit Team

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Response:** `UniResponse<()>` - Deletes team if captain, removes member otherwise

---

### Writeups

#### `GET /api/writeups` - List All Writeups

**Response:** `UniResponse<Vec<ChallengeWriteupResult>>`

---

#### `GET /api/writeups/{writeup_id}` - Get Writeup

**Path Parameters:**
- `writeup_id`: Writeup UUID

**Response:** `UniResponse<ChallengeWriteupResult>`

---

### Submit

#### `POST /api/submit/flag` - Submit Flag

**Request Body:**
```json
{
  "event_id": "uuid|null",      // optional
  "instance_id": "uuid|null",   // optional (for single mode)
  "flag": "string"
}
```

**Response:** `UniResponse<()>` - Empty on success

---

#### `POST /api/submit/writeup` - Submit Writeup

**Multipart Form:**
- `writeup_pdf`: PDF file (max 1GB)
- `event_id`: Event UUID (text)
- `team_id`: Team UUID (optional text)

**Response:** `UniResponse<()>`

---

### Uploads

#### `POST /api/uploads/image` - Upload Image

**Multipart Form:**
- `file`: Image file

**Response:** `UniResponse<String>` - File path

---

## Admin Routes (`/api/admin/*`) - Requires SuperAdmin JWT

### System

#### `GET /api/admin/system/monitor` - Get System Info

**Response:** System monitoring data

---

#### `GET /api/admin/system/changelog` - Get Changelog

**Response:** Changelog data

---

### Database

#### `POST /api/admin/database/exec_sql` - Execute SQL

**Request Body:**
```json
{
  "sql": "string"
}
```

**Response:** Query results

---

### Announcements

#### `GET /api/admin/announcements` - List Announcements

**Query Parameters:**
- `page`: Page number (optional)
- `limit`: Items per page (optional)
- `filter`: JSON filter (optional)

**Response:** `UniResponse<Vec<announcements::Model>>` with meta

---

#### `POST /api/admin/announcements` - Create Announcement

**Request Body:**
```json
{
  "title": "string",
  "content": "string"
}
```

**Response:** `UniResponse<announcements::Model>`

---

#### `DELETE /api/admin/announcements` - Delete Announcements

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/announcements/{announcement_id}` - Update Announcement

**Path Parameters:**
- `announcement_id`: Announcement UUID

**Request Body:**
```json
{
  "title": "string",    // optional
  "content": "string"   // optional
}
```

**Response:** `UniResponse<announcements::Model>`

---

### Settings

#### `GET /api/admin/settings` - List Settings

**Response:** `UniResponse<Vec<settings::Model>>`

---

#### `POST /api/admin/settings` - Create Setting

**Request Body:**
```json
{
  "key": "string",
  "value": "string"
}
```

**Response:** `UniResponse<settings::Model>`

---

#### `DELETE /api/admin/settings` - Delete Settings

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/settings/{setting_id}` - Update Setting

**Path Parameters:**
- `setting_id`: Setting UUID

**Request Body:**
```json
{
  "key": "string",    // optional
  "value": "string"   // optional
}
```

**Response:** `UniResponse<settings::Model>`

---

### Weapons

#### `GET /api/admin/weapons` - List Weapons

**Response:** `UniResponse<Vec<weapons::Model>>`

---

#### `POST /api/admin/weapons` - Create Weapon

**Request Body:**
```json
{
  "name": "string",
  "description": "string"
}
```

**Response:** `UniResponse<weapons::Model>`

---

#### `DELETE /api/admin/weapons` - Delete Weapons

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/weapons/{weapon_id}` - Update Weapon

**Path Parameters:**
- `weapon_id`: Weapon UUID

**Request Body:**
```json
{
  "name": "string",       // optional
  "description": "string" // optional
}
```

**Response:** `UniResponse<weapons::Model>`

---

#### `POST /api/admin/weapons/{weapon_id}/upload` - Upload Weapon File

**Path Parameters:**
- `weapon_id`: Weapon UUID

**Multipart Form:**
- `file`: Weapon file

**Response:** `UniResponse<...>`

---

### Users

#### `GET /api/admin/users` - List Users

**Query Parameters:**
- `page`: Page number (optional)
- `limit`: Items per page (optional)
- `filter`: JSON filter (optional)
  - `id`: Filter by user ID
  - `username`: Filter by username (contains)
  - `nickname`: Filter by nickname (contains)
  - `email`: Filter by email (contains)

**Response:** `UniResponse<Vec<users::Model>>` with meta

---

#### `GET /api/admin/users/{user_id}` - Get User

**Path Parameters:**
- `user_id`: User UUID

**Response:** `UniResponse<users::Model>`

---

#### `POST /api/admin/users` - Create User

**Request Body:**
```json
{
  "username": "string",
  "password": "string",
  "nickname": "string",
  "email": "string"
}
```

**Response:** `UniResponse<users::Model>`

---

#### `DELETE /api/admin/users` - Delete Users

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/users/{user_id}` - Update User

**Path Parameters:**
- `user_id`: User UUID

**Request Body:**
```json
{
  "username": "string",   // optional
  "nickname": "string",   // optional
  "password": "string",   // optional
  "email": "string"       // optional
}
```

**Response:** `UniResponse<users::Model>`

---

### Challenges

#### `GET /api/admin/challenges` - List Challenges

**Query Parameters:**
- `page`: Page number (optional)
- `limit`: Items per page (optional)
- `filter`: JSON filter (optional)
  - `id`: Filter by challenge ID
  - `name`: Filter by name (contains)
  - `category`: Filter by category (contains)
  - `hidden`: Filter by hidden status
  - `description`: Filter by description (contains)

**Response:** `UniResponse<Vec<challenges::Model>>` with meta

---

#### `GET /api/admin/challenges/{challenge_id}` - Get Challenge

**Path Parameters:**
- `challenge_id`: Challenge UUID

**Response:** `UniResponse<challenges::Model>`

---

#### `POST /api/admin/challenges` - Create Challenge

**Request Body:**
```json
{
  "name": "string",
  "category": "string",
  "description": "string",
  "hidden": false,
  "attachment": "string|null",
  "toml_str": "string"
}
```

**Response:** `UniResponse<challenges::Model>`

---

#### `DELETE /api/admin/challenges` - Delete Challenges

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/challenges/{challenge_id}` - Update Challenge

**Path Parameters:**
- `challenge_id`: Challenge UUID

**Request Body:**
```json
{
  "name": "string",        // optional
  "category": "string",   // optional
  "description": "string", // optional
  "attachment": "string",  // optional
  "hidden": true | false, // optional
  "toml_str": "string"    // optional
}
```

**Response:** `UniResponse<challenges::Model>`

---

#### `POST /api/admin/challenges/check` - Check Challenges

Validate challenge Docker images and attachments.

**Request Body:**
```json
{
  "challenge_id_list": ["uuid", ...]  // optional, checks all if empty
}
```

**Response:** `UniResponse<Vec<ChallengeCheckResult>>`
```json
{
  "data": [
    {
      "id": "uuid",
      "challenge_name": "string",
      "is_ok": true | false,
      "docker_image": true | false,
      "attachment": true | false
    }
  ]
}
```

---

#### `POST /api/admin/challenges/import` - Import Challenges

Import challenges from ZIP or base64-encoded TOML.

**Multipart Form:**
- `challenge_zip`: Single challenge ZIP file (optional, max 1GB)
- `challenge_list_zip`: ZIP containing multiple challenges (optional, max 10GB)
- `toml_str_b64`: Base64-encoded TOML string (optional)

**Response:** `UniResponse<Vec<challenges::Model>>`

---

#### `POST /api/admin/challenges/build` - Build Challenge Docker Images

**Request Body:**
```json
{
  "challenge_id": "uuid",           // optional
  "challenge_id_list": ["uuid", ...] // optional
}
```

**Response:** `UniResponse<Vec<BuildChallengeResult>>`
```json
{
  "data": [
    {
      "challenge_name": "string",
      "is_ok": true | false,
      "message": "string"
    }
  ]
}
```

---

### Challenge Sets

#### `GET /api/admin/challenge_sets` - List Challenge Sets

**Response:** `UniResponse<Vec<challenge_sets::Model>>`

---

#### `GET /api/admin/challenge_sets/{challenge_set_id}` - Get Challenge Set

**Path Parameters:**
- `challenge_set_id`: Challenge Set UUID

**Response:** `UniResponse<challenge_sets::Model>`

---

#### `POST /api/admin/challenge_sets` - Create Challenge Set

**Request Body:**
```json
{
  "name": "string",
  "description": "string"
}
```

**Response:** `UniResponse<challenge_sets::Model>`

---

#### `DELETE /api/admin/challenge_sets` - Delete Challenge Sets

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/challenge_sets/{challenge_set_id}` - Update Challenge Set

**Path Parameters:**
- `challenge_set_id`: Challenge Set UUID

**Request Body:**
```json
{
  "name": "string",      // optional
  "description": "string" // optional
}
```

**Response:** `UniResponse<challenge_sets::Model>`

---

#### `POST /api/admin/challenge_sets/{challenge_set_id}/challenges` - Add Challenge to Set

**Path Parameters:**
- `challenge_set_id`: Challenge Set UUID

**Request Body:**
```json
{
  "challenge_id": "uuid"
}
```

**Response:** `UniResponse<()>`

---

#### `DELETE /api/admin/challenge_sets/{challenge_set_id}/challenges` - Remove Challenge from Set

**Path Parameters:**
- `challenge_set_id`: Challenge Set UUID

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

### Super Admin

#### `GET /api/admin/super_admin` - List Super Admins

**Response:** `UniResponse<Vec<super_admin::Model>>`

---

#### `GET /api/admin/super_admin/{super_admin_id}` - Get Super Admin

**Path Parameters:**
- `super_admin_id`: Super Admin UUID

**Response:** `UniResponse<super_admin::Model>`

---

#### `POST /api/admin/super_admin` - Create Super Admin

**Request Body:**
```json
{
  "username": "string",
  "password": "string",
  "email": "string"
}
```

**Response:** `UniResponse<super_admin::Model>`

---

#### `DELETE /api/admin/super_admin` - Delete Super Admins

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `POST /api/admin/super_admin/{super_admin_id}` - Update Super Admin

**Path Parameters:**
- `super_admin_id`: Super Admin UUID

**Request Body:**
```json
{
  "username": "string",   // optional
  "password": "string",   // optional
  "email": "string"      // optional
}
```

**Response:** `UniResponse<super_admin::Model>`

---

### Instances

#### `GET /api/admin/instances` - List Instances

**Response:** `UniResponse<Vec<instances::Model>>`

---

#### `GET /api/admin/instances/{instance_id}` - Get Instance

**Path Parameters:**
- `instance_id`: Instance UUID

**Response:** `UniResponse<instances::Model>`

---

### Events

#### `GET /api/admin/events` - List Events

**Query Parameters:**
- `page`: Page number (optional)
- `limit`: Items per page (optional)
- `filter`: JSON filter (optional)
  - `id`: Filter by event ID
  - `type`: Filter by event type
  - `title`: Filter by title (contains)
  - `hidden`: Filter by hidden status
  - `allow_join`: Filter by allow_join status

**Response:** `UniResponse<Vec<events::Model>>` with meta

---

#### `GET /api/admin/events/{event_id}` - Get Event

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<events::Model>`

---

#### `POST /api/admin/events` - Create Event

**Request Body:**
```json
{
  "type": "JeopardyPractice | JeopardySingle | JeopardyTeam",
  "title": "string",
  "description": "string|null",
  "hidden": false,
  "allow_join": true | false,
  "rules": "string",
  "start_time": "datetime",
  "end_time": "datetime"
}
```

**Response:** `UniResponse<events::Model>`

---

#### `DELETE /api/admin/events` - Delete Events

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/events/{event_id}` - Update Event

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "type": "JeopardyPractice | JeopardySingle | JeopardyTeam", // optional
  "title": "string",   // optional
  "description": "string",  // optional
  "hidden": true | false,   // optional
  "allow_join": true | false, // optional
  "rules": "string",   // optional
  "flag_prefix": "string", // optional
  "start_time": "datetime", // optional
  "end_time": "datetime" // optional
}
```

**Response:** `UniResponse<events::Model>`

---

#### `GET /api/admin/events/{event_id}/data` - Get Event Dashboard Data

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<DataPresent>`
```json
{
  "data": {
    "event": {...},
    "user_count": 100,
    "team_count": 20,
    "solved_recent_15": [...],
    "event_challenges": [...],
    "scoreboard_top10": [...],
    "trend": [...]
  }
}
```

---

#### `GET /api/admin/events/{event_id}/report` - Generate Event Report

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<String>` - ZIP file path containing report.html and writeups

---

### Event Users

#### `GET /api/admin/events/{event_id}/users` - List Event Users

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<event_users::Model>>`

---

#### `POST /api/admin/events/{event_id}/users` - Add User to Event

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "user_id": "uuid"
}
```

**Response:** `UniResponse<event_users::Model>`

---

#### `DELETE /api/admin/events/{event_id}/users` - Remove Users from Event

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `POST /api/admin/events/{event_id}/users/{user_id}/banned` - Ban User

**Path Parameters:**
- `event_id`: Event UUID
- `user_id`: User UUID

**Response:** `UniResponse<()>`

---

#### `POST /api/admin/events/{event_id}/users/{user_id}/unbanned` - Unban User

**Path Parameters:**
- `event_id`: Event UUID
- `user_id`: User UUID

**Response:** `UniResponse<()>`

---

### Event Teams

#### `GET /api/admin/events/{event_id}/teams` - List Event Teams

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<event_teams::Model>>`

---

#### `GET /api/admin/events/{event_id}/teams/{team_id}/members` - Get Team Members

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Response:** `UniResponse<Vec<TeamMemberResult>>`

---

#### `POST /api/admin/events/{event_id}/teams` - Add Team

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "name": "string"
}
```

**Response:** `UniResponse<event_teams::Model>`

---

#### `DELETE /api/admin/events/{event_id}/teams` - Remove Teams

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `POST /api/admin/events/{event_id}/teams/{team_id}/users` - Add User to Team

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Request Body:**
```json
{
  "user_id": "uuid"
}
```

**Response:** `UniResponse<()>`

---

#### `DELETE /api/admin/events/{event_id}/teams/{team_id}/users` - Remove User from Team

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `POST /api/admin/events/{event_id}/teams/{team_id}/banned` - Ban Team

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Response:** `UniResponse<()>`

---

#### `POST /api/admin/events/{event_id}/teams/{team_id}/unbanned` - Unban Team

**Path Parameters:**
- `event_id`: Event UUID
- `team_id`: Team UUID

**Response:** `UniResponse<()>`

---

### Event Challenges

#### `GET /api/admin/events/{event_id}/challenges` - List Event Challenges

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<event_challenges::Model>>`

---

#### `POST /api/admin/events/{event_id}/challenges` - Add Challenge to Event

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "challenge_id": "uuid",
  "points": 500.0
}
```

**Response:** `UniResponse<event_challenges::Model>`

---

#### `DELETE /api/admin/events/{event_id}/challenges` - Remove Challenge from Event

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `POST /api/admin/events/{event_id}/challenges/hidden` - Hide Challenges

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "challenge_id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<()>`

---

#### `POST /api/admin/events/{event_id}/challenges/open` - Open Challenges

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "challenge_id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<()>`

---

### Event Announcements

#### `GET /api/admin/events/{event_id}/announcements` - List Event Announcements

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<event_announcements::Model>>`

---

#### `GET /api/admin/events/{event_id}/announcements/{announcement_id}` - Get Event Announcement

**Path Parameters:**
- `event_id`: Event UUID
- `announcement_id`: Announcement UUID

**Response:** `UniResponse<event_announcements::Model>`

---

#### `POST /api/admin/events/{event_id}/announcements` - Create Event Announcement

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "title": "string",
  "content": "string"
}
```

**Response:** `UniResponse<event_announcements::Model>`

---

#### `PATCH /api/admin/events/{event_id}/announcements/{announcement_id}` - Update Event Announcement

**Path Parameters:**
- `event_id`: Event UUID
- `announcement_id`: Announcement UUID

**Request Body:**
```json
{
  "title": "string",   // optional
  "content": "string"  // optional
}
```

**Response:** `UniResponse<event_announcements::Model>`

---

#### `DELETE /api/admin/events/{event_id}/announcements` - Remove Event Announcements

**Path Parameters:**
- `event_id`: Event UUID

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

### Event Writeups

#### `GET /api/admin/events/{event_id}/writeups` - List All Event Writeups

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<event_writeup::Model>>`

---

### Event Logs

#### `GET /api/admin/events/{event_id}/logs` - Get Event Logs

**Path Parameters:**
- `event_id`: Event UUID

**Response:** `UniResponse<Vec<logs::Model>>`

---

### Scheduled Tasks

#### `GET /api/admin/scheduled_tasks` - List Scheduled Tasks

**Response:** `UniResponse<Vec<scheduled_tasks::Model>>`

---

#### `GET /api/admin/scheduled_tasks/{task_id}` - Get Scheduled Task

**Path Parameters:**
- `task_id`: Task UUID

**Response:** `UniResponse<scheduled_tasks::Model>`

---

#### `POST /api/admin/scheduled_tasks` - Create Scheduled Task

**Request Body:**
```json
{
  "name": "string",
  "cron": "string",
  "action": "string"
}
```

**Response:** `UniResponse<scheduled_tasks::Model>`

---

#### `DELETE /api/admin/scheduled_tasks` - Delete Scheduled Tasks

**Request Body:**
```json
{
  "id_list": ["uuid", ...]
}
```

**Response:** `UniResponse<u64>` - Deleted count

---

#### `PATCH /api/admin/scheduled_tasks/{task_id}` - Update Scheduled Task

**Path Parameters:**
- `task_id`: Task UUID

**Request Body:**
```json
{
  "name": "string",   // optional
  "cron": "string",   // optional
  "action": "string"  // optional
}
```

**Response:** `UniResponse<scheduled_tasks::Model>`

---

### Logs

#### `GET /api/admin/logs` - List Logs

**Response:** `UniResponse<Vec<logs::Model>>`

---

#### `GET /api/admin/logs/{log_id}` - Get Log

**Path Parameters:**
- `log_id`: Log UUID

**Response:** `UniResponse<logs::Model>`

---

## Data Models

### challenges::Model

| Field | Type | Description |
|-------|------|-------------|
| id | Uuid | Primary key |
| name | String | Challenge name (unique) |
| safe_name | String | URL-safe name (unique) |
| category | String | Challenge category |
| description | String | Challenge description |
| attachment | Option<String> | Attachment filename |
| hidden | bool | Whether challenge is hidden |
| toml_str | String | Challenge configuration TOML |
| created_at | DateTimeWithTimeZone | Creation timestamp |
| updated_at | DateTimeWithTimeZone | Last update timestamp |

### events::Model

| Field | Type | Description |
|-------|------|-------------|
| id | Uuid | Primary key |
| type | EventType | Event type enum |
| title | String | Event title |
| description | Option<String> | Event description |
| hidden | bool | Whether event is hidden |
| allow_join | bool | Whether users can join |
| rules | String | Event rules |
| start_time | DateTimeWithTimeZone | Start time |
| end_time | DateTimeWithTimeZone | End time |
| flag_prefix | Option<String> | Custom flag prefix |

### EventType Enum

| Value | Description |
|-------|-------------|
| `JeopardyPractice` | Practice mode (solo) |
| `JeopardySingle` | Competitive single-player |
| `JeopardyTeam` | Competitive team-based |

### instances::Model

| Field | Type | Description |
|-------|------|-------------|
| id | Uuid | Primary key |
| challenge_id | Uuid | Related challenge |
| user_id | Uuid | Instance owner |
| gamebox_id | Option<Uuid> | Gamebox reference |
| status | InstanceStatus | Running/Stopped/Error |
| flag | String | Instance flag |
| ref | String | Reference type (JeopardyPractice, event_id, etc.) |
| created_at | DateTimeWithTimeZone | Creation timestamp |
| updated_at | DateTimeWithTimeZone | Last update timestamp |

### InstanceStatus Enum

| Value | Description |
|-------|-------------|
| `Running` | Instance is running |
| `Stopped` | Instance is stopped |
| `Error` | Instance error state |

---

## Query Parameters

All list endpoints support pagination and filtering:

### Pagination

| Parameter | Type | Description |
|-----------|------|-------------|
| page | usize | Page number (1-indexed) |
| limit | usize | Items per page |

### Filtering

| Parameter | Type | Description |
|-----------|------|-------------|
| filter | JSON string | Filter expression |

### Filter Format

```json
{
  "key": "value",
  "key2": "value2"
}
```

Common filter keys:
- `id`: UUID exact match
- `name`: String contains
- `category`: String contains
- `type`: Enum value
- `hidden`: Boolean
- `allow_join`: Boolean

---

## Response Format

All responses follow `UniResponse<T>` format:

```json
{
  "data": T,
  "meta": {
    "page": 1,
    "limit": 10,
    "total": 100
  }
}
```

For endpoints returning no data (`UniResponse<()>`):
```json
{
  "data": null
}
```

---

## Error Responses

Errors return `UniError` with HTTP status codes:

| Status | Description |
|--------|-------------|
| 400 | Bad Request |
| 401 | Unauthorized (AuthError) |
| 404 | Not Found |
| 500 | Internal Server Error |

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human readable message"
  }
}
```

---

## Dynamic Score Calculation

Events use dynamic scoring that decays based on solve count:

```
score = min_points + (base_points - min_points) * sqrt(decay / (decay + solves))
```

Where:
- `min_points = base_points * event_score_min_percent`
- `decay` and `event_score_min_percent` are system settings


## License
本项目以 [GNU AGPLv3](LICENSE) 协议发布。
Copyright (C) 2025-2026 fb0sh@outlook.com
