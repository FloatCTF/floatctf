//! Challenge catalog — player/admin CRUD and practice solves.

pub mod dto;
pub use dto::{ChallengeAttachmentDto, ChallengeRevisionDto, ChallengesDto};

pub mod admin;
pub mod player;
pub mod solves;
