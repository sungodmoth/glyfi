use std::{char, collections::HashMap, str::FromStr, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use poise::serenity_prelude::{prelude::TypeMapKey, ChannelId, Emoji, EmojiId, MessageId, ReactionType, UserId};
use sqlx::prelude::FromRow;
use tokio::sync::RwLock;

use crate::{server_data::{AMBIGRAM_ANNOUNCEMENTS_CHANNEL_ID, AMBI_INTERVAL, GLYPH_ANNOUNCEMENTS_CHANNEL_ID, GLYPH_INTERVAL}, Error};

/// Data associated with a given glyph/ambi prompt
#[derive(Clone, Debug, PartialEq)]
pub struct PromptData {
    pub challenge: Challenge,
    pub prompt: String,
    pub size_percentage: Option<u16>,
    pub custom_duration: Option<u16>,
    pub is_special: Option<bool>,
    pub extra_announcement_text: Option<String>,
}

/// What challenge a submission belongs to.
#[derive(Copy, Clone, Debug, PartialEq, poise::ChoiceParameter)]
#[repr(u8)]
pub enum Challenge {
    Glyph = 0,
    Ambigram = 1,
}

impl Challenge {
    pub fn raw(self) -> u8 {
        self as _
    }

    pub fn short_name(&self) -> String {
        match self {
            Challenge::Glyph => "glyph".to_owned(),
            Challenge::Ambigram => "ambi".to_owned(),
        }
    }

    pub fn long_name(&self) -> String {
        match self {
            Challenge::Glyph => "glyph".to_owned(),
            Challenge::Ambigram => "ambigram".to_owned(),
        }
    }

    pub fn one_char_name(&self) -> char {
        match self {
            Challenge::Glyph => 'g',
            Challenge::Ambigram => 'a',
        }
    }

    pub fn name_to_path(s: &str) -> String {
        format!("./generation/{}.png", s)
    }

    pub fn default_duration(&self) -> Duration {
        match self {
            Challenge::Glyph => GLYPH_INTERVAL,
            Challenge::Ambigram => AMBI_INTERVAL,
        }
    }

    pub fn announcement_channel(&self) -> ChannelId {
        match self {
            Challenge::Glyph => GLYPH_ANNOUNCEMENTS_CHANNEL_ID,
            Challenge::Ambigram => AMBIGRAM_ANNOUNCEMENTS_CHANNEL_ID
        }
    }
    
}

impl FromStr for Challenge {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(Challenge::Glyph),
            "1" => Ok(Challenge::Ambigram),
            id => Err(format!("Unknown challenge ID '{:?}'", id).into()),
        }
    }
}

impl From<i64> for Challenge {
    fn from(i: i64) -> Self {
        match i {
            0 => Challenge::Glyph,
            1 => Challenge::Ambigram,
            _ => panic!("Invalid challenge ID {}", i),
        }
    }
}

/// Determines what kind of actions should be taken in a week.
///
/// Every week, we need to perform the following actions for
/// each challenge:
///
/// - Make an announcement post that describes that week’s challenge.
/// - Post a panel containing all submissions from the previous week.
/// - Post the top 3 submissions from the week before that.
///
/// Some weeks, however, are special in that we don’t want to take
/// one or more of those actions. A week can either be ‘regular’ or
/// ‘special’.
///
/// At the ‘beginning’ of the week (that is, the day the announcement
/// is made) we need to:
///
/// - Make a new announcement post for the current week, unless this
///   week is special.
///
/// - Post a panel containing all submissions from the previous week,
///   unless that week was special.
///
/// - Post the top three from the week before the last.

/// Profile for a user.
#[derive(Clone, Debug)]
pub struct UserProfileData {
    pub nickname: Option<String>,

    /// Number of 1st, 2nd, 3rd place finishes in the Glyphs Challenge.
    pub glyphs_first: i64,
    pub glyphs_second: i64,
    pub glyphs_third: i64,

    /// Number of 1st, 2nd, 3rd place finishes in the Ambigram Challenge.
    pub ambigrams_first: i64,
    pub ambigrams_second: i64,
    pub ambigrams_third: i64,

    /// Highest ranking in either challenge.
    pub highest_ranking_glyphs: i64,
    pub highest_ranking_ambigrams: i64,

    /// Number of submissions.
    pub glyphs_submissions: i64,
    pub ambigrams_submissions: i64,
}

#[derive(Clone, Debug)]
pub struct WeekInfo {
    pub challenge: Challenge,
    pub week: i64,
    pub prompt: String,
    pub size_percentage: u16,
    pub target_start_time: DateTime<Utc>,
    pub target_end_time: DateTime<Utc>,
    pub actual_start_time: DateTime<Utc>,
    pub actual_end_time: DateTime<Utc>,
    pub is_special: bool,
    pub num_subs: i64,
    // there's not a significantly better way to handle this on the database side...
    // if we want to be able to support more than 40 submissions, we'll just have
    // to add a third column
    pub poll_message_id: Option<MessageId>,
    pub second_poll_message_id: Option<MessageId>
}

#[derive(Clone, Debug)]
pub enum WinnerPosition {
    First,
    Second,
    Third
}

impl WinnerPosition {
    pub fn name(&self) -> String {
        match self {
            Self::First => "first".to_owned(),
            Self::Second => "second".to_owned(),
            Self::Third => "third".to_owned()
        }
    }
}


#[derive(Clone, Debug)]
pub enum ChallengeImageOptions {
    Announcement{prompt: String, size_percentage: u16},
    Poll{prompt: String, size_percentage: u16},
    Winner{position: WinnerPosition, winner_nick: String, winner_id: UserId, submission_id: MessageId},
}

impl ChallengeImageOptions {
    pub fn suffix(&self) -> String {
        match self {
            Self::Announcement { .. } => "announcement".to_owned(),
            Self::Poll { .. } => "poll".to_owned(),
            Self::Winner { position, .. } => position.name() 
        }
    }
}

/// The types of image which we might want to preview.
#[derive(Clone, Debug, poise::ChoiceParameter)]
pub enum PreviewableImages {
    #[name="next_challenge_announcement"]
    Announcement,
    #[name="this_challenge_poll"]
    Poll,
    #[name="this_challenge_first_place"]
    FirstPlace,
    #[name="this_challenge_second_place"]
    SecondPlace,
    #[name="this_challenge_third_place"]
    ThirdPlace,
}

#[derive(Clone, Debug, poise::ChoiceParameter)]
pub enum UploadableImages {
    #[name="next_challenge_announcement"]
    Announcement,
    #[name="this_challenge_poll"]
    Poll,
}

#[derive(Copy, Clone, Debug)]
pub enum AnyEmoji {
    Default(char),
    Custom(EmojiId, &'static str)
}
impl Into<ReactionType> for AnyEmoji {
    fn into(self) -> ReactionType {
        match self {
            Self::Default(ch) => ch.into(),
            Self::Custom(id, name) => ReactionType::Custom { animated: false, id, name: Some(name.to_owned()) }
        }
    }
}
impl PartialEq::<ReactionType> for AnyEmoji {
    fn eq(&self, other: &ReactionType) -> bool {
        match (self, other) {
            (Self::Default(s1), ReactionType::Unicode(s2)) => s1.to_string() == *s2,
            (Self::Custom(id1, name1), ReactionType::Custom { id: id2, name: name2, .. }) => id1 == id2,
            _ => false 
        }
    }
}
impl AnyEmoji {
    pub fn display_string(&self) -> String {
        match self {
            AnyEmoji::Default(chr) => chr.to_string(),
            AnyEmoji::Custom(id, name) => format!("<:{}:{}>", name, id).to_string()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UserVoteReplyStatus {
    Idle,
    Waiting(i64),
    Responding
}

#[derive(Clone, Debug, PartialEq)]
pub struct UserVoteStatusData;

impl TypeMapKey for UserVoteStatusData {
    type Value = Arc<RwLock<HashMap<UserId, UserVoteReplyStatus>>>;
}