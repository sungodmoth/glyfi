use std::{char, collections::HashMap, ops::{Add, AddAssign}, str::FromStr, sync::Arc};

use chrono::{DateTime, Duration, TimeDelta, Utc};
use poise::serenity_prelude::{prelude::TypeMapKey, ChannelId, Emoji, EmojiId, MessageId, ReactionType, UserId};
use sqlx::{prelude::FromRow, sqlite::SqliteRow};
use tokio::sync::RwLock;

use crate::{server_data::{AMBIGRAM_ANNOUNCEMENTS_CHANNEL_ID, AMBI_INTERVAL, GLYPH_ANNOUNCEMENTS_CHANNEL_ID, GLYPH_INTERVAL}, Error, ResT};

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MsgId(pub Option<MessageId>);

impl From<Option<MessageId>> for MsgId {
    fn from(value: Option<MessageId>) -> Self {
        Self(value)
    }
}
impl TryFrom<i64> for MsgId {
    type Error = ();
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Ok(Some(value as u64).filter(|x| *x != 0).map(|x| x.into()).into())
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Timestamp(pub DateTime<Utc>);

impl From<DateTime<Utc>> for Timestamp {
    fn from(value: DateTime<Utc>) -> Self {
        Self(value)
    }
}

impl Add<TimeDelta> for Timestamp {
    type Output = Timestamp;
    fn add(self, rhs: TimeDelta) -> Self::Output {
        (self.0 + rhs).into()
    }
}

impl AddAssign<TimeDelta> for Timestamp {
    fn add_assign(&mut self, rhs: TimeDelta) {
        *self = *self + rhs;
    }
}

impl TryFrom<i64> for Timestamp {
    type Error = Error;
    fn try_from(value: i64) -> ResT<Self> {
        DateTime::<Utc>::from_timestamp(value, 0).ok_or("Error parsing unix timestamp.".into()).map(|x| x.into())
    }
}

/// Data associated with a given glyph/ambi prompt
#[derive(Clone, Debug, PartialEq, FromRow)]
pub struct PromptData {
    #[sqlx(try_from="i8")]
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

impl TryFrom<i8> for Challenge {
    type Error = ();
    fn try_from(i: i8) -> Result<Self, Self::Error> {
        match i {
            0 => Ok(Challenge::Glyph),
            1 => Ok(Challenge::Ambigram),
            _ => Err(()),
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

#[derive(Clone, Debug, FromRow)]
pub struct WeekInfo {
    #[sqlx(try_from="i8")]
    pub challenge: Challenge,
    pub week: i64,
    pub prompt: String,
    pub size_percentage: u16,
    #[sqlx(try_from="i64")]
    pub target_start_time: Timestamp,
    #[sqlx(try_from="i64")]
    pub target_end_time: Timestamp,
    #[sqlx(try_from="i64")]
    pub actual_start_time: Timestamp,
    #[sqlx(try_from="i64")]
    pub actual_end_time: Timestamp,
    pub is_special: bool,
    pub num_subs: i64,
    #[sqlx(try_from="i64")]
    pub poll_message_id: MsgId,
    #[sqlx(try_from="i64")]
    pub second_poll_message_id: MsgId,
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