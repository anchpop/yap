//! Profile and social endpoints of the AI backend, exposed to every host.
use crate::utils::hit_ai_server;
use bridgerton::Error;
use language_utils::profile::{
    FollowRequest, FollowResponse, FollowStatus, Profile, UpdateProfileRequest,
    UpdateProfileResponse, UserLanguageStats,
};

/// Perform a request against the AI backend and decode its JSON body.
async fn fetch_json<T: serde::de::DeserializeOwned>(
    method: fetch_happen::Method,
    path: &str,
    request: Option<impl serde::Serialize>,
    access_token: Option<&String>,
) -> Result<T, Error> {
    let response = hit_ai_server(method, path, request, access_token)
        .await
        .map_err(|e| Error::new(format!("Request error: {e:?}")))?;

    if !response.ok() {
        return Err(Error::new(format!("HTTP error: {}", response.status())));
    }

    response
        .json()
        .await
        .map_err(|e| Error::new(format!("Response parsing error: {e:?}")))
}

#[bridgerton::bridge]
pub async fn get_profile_by_id(user_id: String) -> Result<Profile, Error> {
    fetch_json(
        fetch_happen::Method::GET,
        &format!("/profile?id={user_id}"),
        None::<()>,
        None,
    )
    .await
}

#[bridgerton::bridge]
pub async fn get_profile_by_slug(slug: String) -> Result<Profile, Error> {
    fetch_json(
        fetch_happen::Method::GET,
        &format!("/profile?slug={slug}"),
        None::<()>,
        None,
    )
    .await
}

#[bridgerton::bridge]
pub async fn update_profile(
    display_name: Option<String>,
    bio: Option<String>,
    access_token: String,
) -> Result<UpdateProfileResponse, Error> {
    let request = UpdateProfileRequest { display_name, bio };
    fetch_json(
        fetch_happen::Method::PATCH,
        "/profile",
        Some(&request),
        Some(&access_token),
    )
    .await
}

#[bridgerton::bridge]
pub async fn get_user_language_stats_by_id(
    user_id: String,
) -> Result<Vec<UserLanguageStats>, Error> {
    fetch_json(
        fetch_happen::Method::GET,
        &format!("/user-language-stats?id={user_id}"),
        None::<()>,
        None,
    )
    .await
}

#[bridgerton::bridge]
pub async fn get_user_language_stats_by_slug(
    slug: String,
) -> Result<Vec<UserLanguageStats>, Error> {
    fetch_json(
        fetch_happen::Method::GET,
        &format!("/user-language-stats?slug={slug}"),
        None::<()>,
        None,
    )
    .await
}

#[bridgerton::bridge]
pub async fn follow_user(user_id: String, access_token: String) -> Result<FollowResponse, Error> {
    let request = FollowRequest { user_id };
    fetch_json(
        fetch_happen::Method::POST,
        "/follow",
        Some(&request),
        Some(&access_token),
    )
    .await
}

#[bridgerton::bridge]
pub async fn unfollow_user(user_id: String, access_token: String) -> Result<FollowResponse, Error> {
    let request = FollowRequest { user_id };
    fetch_json(
        fetch_happen::Method::POST,
        "/unfollow",
        Some(&request),
        Some(&access_token),
    )
    .await
}

#[bridgerton::bridge]
pub async fn get_follow_status(
    user_id: Option<String>,
    slug: Option<String>,
    access_token: String,
) -> Result<FollowStatus, Error> {
    let query = if let Some(id) = user_id {
        format!("id={id}")
    } else if let Some(s) = slug {
        format!("slug={s}")
    } else {
        return Err(Error::new("Either user_id or slug must be provided"));
    };
    fetch_json(
        fetch_happen::Method::GET,
        &format!("/follow-status?{query}"),
        None::<()>,
        Some(&access_token),
    )
    .await
}
