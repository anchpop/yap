use crate::Language;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct GetProfileQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct UpdateProfileRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
}

#[bridgerton::bridge(transparent)]
#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct UpdateProfileResponse {
    pub success: bool,
}

#[bridgerton::bridge(transparent)]
#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct Profile {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name_slug: Option<String>,
    pub notifications_enabled: bool,
    pub email_notifications_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[bridgerton::bridge(transparent)]
#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct UserLanguageStats {
    pub user_id: String,
    pub language: Language,
    pub total_count: i64,
    pub daily_streak: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_streak_expiry: Option<String>,
    pub xp: f64,
    pub percent_known: f64,
    pub started: String,
    pub last_updated: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct UpdateLanguageStatsRequest {
    pub language: Language,
    pub total_count: i64,
    pub daily_streak: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_streak_expiry: Option<String>,
    pub xp: f64,
    pub percent_known: f64,
    pub start_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct UpdateLanguageStatsResponse {
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct FollowRequest {
    pub user_id: String,
}

#[bridgerton::bridge(transparent)]
#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct FollowResponse {
    pub success: bool,
}

#[bridgerton::bridge(transparent)]
#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct FollowStatus {
    pub is_following: bool,
    pub follower_count: i64,
    pub following_count: i64,
}
