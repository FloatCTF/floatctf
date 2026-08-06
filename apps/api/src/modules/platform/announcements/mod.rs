//! Site-wide announcements (not event announcements).

mod api;
mod dto;

pub use api::{
    create_announcement, delete_announcement, get_announcements, get_player_announcements,
    patch_announcement,
};
pub use dto::AnnouncementsDto;
