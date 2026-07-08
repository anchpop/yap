use crate::{CardSummary, Deck, supabase::supabase_config};
use chrono::Utc;
use wasm_bindgen::prelude::*;
use weapon::supabase::SupabaseConfig;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NotificationType {
    DueNow,
    MorningReminder,
    AfternoonReminder,
    EveningReminder,
    EncourageNewCards,
    EncourageNewCards3d,
    WeeklyCheckpoint,
    WeeklyForecast,
    BiweeklyForecast,
    MonthlyMilestone,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub(crate) struct ScheduledNotification {
    pub(crate) scheduled_at: f64, // timestamp in milliseconds
    pub(crate) notification: Notification,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub(crate) struct Notification {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) notification_type: NotificationType,
}

impl NotificationType {
    pub(crate) fn show(&self, due_cards: &[&CardSummary]) -> Option<Notification> {
        match self {
            NotificationType::DueNow => (!due_cards.is_empty()).then(|| Notification {
                title: "Time to study! 📚".to_string(),
                body: "Your next card is ready for review".to_string(),
                notification_type: self.clone(),
            }),
            NotificationType::MorningReminder => (!due_cards.is_empty()).then(|| Notification {
                title: "Good morning! ☀️".to_string(),
                body: format!(
                    "{} card{} to review today",
                    due_cards.len(),
                    if due_cards.len() == 1 { "" } else { "s" }
                ),
                notification_type: self.clone(),
            }),
            NotificationType::AfternoonReminder => (due_cards.len() > 2).then(|| Notification {
                title: "Afternoon study break? ☕".to_string(),
                body: "Perfect time for a quick review session!".to_string(),
                notification_type: self.clone(),
            }),
            NotificationType::EveningReminder => (!due_cards.is_empty()).then(|| Notification {
                title: "Evening review time 🌙".to_string(),
                body: format!(
                    "Wrap up the day with {} card{}",
                    due_cards.len(),
                    if due_cards.len() == 1 { "" } else { "s" }
                ),
                notification_type: self.clone(),
            }),
            NotificationType::EncourageNewCards => due_cards.is_empty().then(|| Notification {
                title: "Keep the momentum going! 🚀".to_string(),
                body: "You're all caught up! Time to learn some new words?".to_string(),
                notification_type: self.clone(),
            }),
            NotificationType::EncourageNewCards3d => due_cards.is_empty().then(|| Notification {
                title: "Ready for a challenge? 💪".to_string(),
                body: "Add 5 new words to keep your learning streak alive!".to_string(),
                notification_type: self.clone(),
            }),
            NotificationType::WeeklyCheckpoint => (!due_cards.is_empty()).then(|| Notification {
                title: "Stay on track! 📊".to_string(),
                body: format!(
                    "{} cards coming up this week - you've got this!",
                    due_cards.len()
                ),
                notification_type: self.clone(),
            }),
            NotificationType::WeeklyForecast => (!due_cards.is_empty()).then(|| Notification {
                title: "Weekly review forecast 📅".to_string(),
                body: format!("{} cards scheduled for the next week", due_cards.len()),
                notification_type: self.clone(),
            }),
            NotificationType::BiweeklyForecast => (!due_cards.is_empty()).then(|| Notification {
                title: "Two-week outlook 🔮".to_string(),
                body: format!("{} cards coming up in the next month", due_cards.len()),
                notification_type: self.clone(),
            }),
            NotificationType::MonthlyMilestone => (!due_cards.is_empty()).then(|| Notification {
                title: "Ready to start again? 🏆".to_string(),
                body: "This will be your last Yap notification until you start Yapping again."
                    .to_string(),
                notification_type: self.clone(),
            }),
        }
    }
}

impl Notification {
    pub fn at(self, scheduled_at: f64) -> ScheduledNotification {
        ScheduledNotification {
            scheduled_at,
            notification: self,
        }
    }
}

impl Deck {
    pub(crate) fn compute_scheduled_notifications(
        &self,
        timezone_offset_minutes: i32,
    ) -> Vec<ScheduledNotification> {
        let mut notifications = Vec::new();
        let now = Utc::now();
        let now_millis = now.timestamp_millis() as f64;

        // Convert UTC to user's local time using the offset
        // Note: getTimezoneOffset returns positive when local is behind UTC,
        // so we need to subtract it to get local time
        let local_now = now - chrono::Duration::minutes(timezone_offset_minutes as i64);

        // Get all cards sorted by due date
        let cards = self.get_all_cards_summary();

        // Find cards that are due
        let due_cards: Vec<&CardSummary> = cards
            .iter()
            .filter(|card| card.due_timestamp_ms <= now_millis)
            .collect();

        // Helper function to get a specific hour today or in the future
        let get_next_occurrence = |hour: u32, days_ahead: i64| {
            // Get the target date in the user's local time
            let local_target = (local_now + chrono::Duration::days(days_ahead))
                .date_naive()
                .and_hms_opt(hour, 0, 0)
                .unwrap();

            // Convert from local time to UTC by adding the timezone offset
            // (opposite of the conversion to local time)
            local_target.and_utc() + chrono::Duration::minutes(timezone_offset_minutes as i64)
        };

        // 1. Notification for when next card becomes due
        if due_cards.is_empty() {
            // Find the next card that will become due
            let next_due_card = cards
                .iter()
                .filter(|card| card.due_timestamp_ms > now_millis)
                .min_by_key(|card| card.due_timestamp_ms as i64);

            if let Some(next_card) = next_due_card
                && let Some(notification) = NotificationType::DueNow.show(&[next_card])
            {
                notifications.push(notification.at(next_card.due_timestamp_ms));
            }
        }

        // First, determine all potential notification times
        let mut notification_times: std::collections::BTreeMap<i64, NotificationType> =
            std::collections::BTreeMap::new();

        let cards_due_by = |time_millis: i64| -> Vec<&CardSummary> {
            cards
                .iter()
                .filter(|card| card.due_timestamp_ms <= time_millis as f64)
                .collect()
        };

        // Today's notification times
        notification_times.insert(
            get_next_occurrence(9, 0).timestamp_millis(),
            NotificationType::MorningReminder,
        );
        notification_times.insert(
            get_next_occurrence(15, 0).timestamp_millis(),
            NotificationType::AfternoonReminder,
        );
        notification_times.insert(
            get_next_occurrence(20, 0).timestamp_millis(),
            NotificationType::EveningReminder,
        );

        // Future notification times
        notification_times.insert(
            get_next_occurrence(19, 1).timestamp_millis(),
            NotificationType::EncourageNewCards,
        );
        notification_times.insert(
            get_next_occurrence(18, 3).timestamp_millis(),
            NotificationType::EncourageNewCards3d,
        );
        notification_times.insert(
            get_next_occurrence(19, 3).timestamp_millis(),
            NotificationType::WeeklyCheckpoint,
        );
        notification_times.insert(
            get_next_occurrence(19, 7).timestamp_millis(),
            NotificationType::WeeklyForecast,
        );
        notification_times.insert(
            get_next_occurrence(19, 14).timestamp_millis(),
            NotificationType::BiweeklyForecast,
        );
        notification_times.insert(
            get_next_occurrence(19, 30).timestamp_millis(),
            NotificationType::MonthlyMilestone,
        );

        for (&time, notification_type) in notification_times.iter() {
            // Skip notifications scheduled in the past — these would fire immediately
            // via the cron job, causing spurious notifications every time the app is opened.
            if time <= now.timestamp_millis() {
                continue;
            }

            let cards = cards_due_by(time);
            let time_f64 = time as f64;

            // Find the notification type for this time
            if let Some(notification) = notification_type.show(&cards) {
                notifications.push(notification.at(time_f64));
            }
        }

        // Remove duplicate notifications at the same time
        notifications.sort_by(|a, b| a.scheduled_at.total_cmp(&b.scheduled_at));
        notifications.dedup_by(|a, b| (a.scheduled_at - b.scheduled_at).abs() < 60000.0); // Within 1 minute

        notifications
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Deck {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn submit_push_notifications(
        &self,
        access_token: &str,
        user_id: &str,
    ) -> Result<(), JsValue> {
        let client = fetch_happen::Client;

        let SupabaseConfig {
            supabase_url,
            supabase_anon_key,
        } = supabase_config();

        // Get timezone offset from JS
        let timezone_offset = js_sys::Date::new_0().get_timezone_offset();
        let scheduled_notifications = self.compute_scheduled_notifications(timezone_offset as i32);

        // Convert to JSON values for API with proper timestamp formatting
        let notifications_json: Vec<serde_json::Value> = scheduled_notifications
                .into_iter()
                .map(|n| {
                    serde_json::json!({
                        "user_id": user_id,
                        "scheduled_at": chrono::DateTime::<chrono::Utc>::from_timestamp_millis(n.scheduled_at as i64)
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default(),
                        "title": n.notification.title,
                        "body": n.notification.body,
                        "notification_type": n.notification.notification_type,
                        "sent": false
                    })
                })
                .collect();

        if !notifications_json.is_empty() {
            log::info!(
                "Updating {} scheduled notifications",
                notifications_json.len()
            );

            // First delete existing notifications
            let delete_url =
                format!("{supabase_url}/rest/v1/scheduled_notifications?user_id=eq.{user_id}");

            let _ = client
                .delete(&delete_url)
                .header("apikey", &supabase_anon_key)
                .header("Authorization", format!("Bearer {access_token}"))
                .send()
                .await;

            // Insert new notifications
            let insert_url = format!("{supabase_url}/rest/v1/scheduled_notifications");

            let insert_response = client
                .post(&insert_url)
                .header("apikey", &supabase_anon_key)
                .header("Authorization", format!("Bearer {access_token}"))
                .json(&notifications_json)
                .map_err(|e| JsValue::from_str(&format!("{e:?}")))?
                .send()
                .await
                .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

            if !insert_response.ok() {
                log::warn!(
                    "Failed to update notifications: {}",
                    insert_response.status()
                );
            } else {
                log::info!("Successfully updated notifications");
            }
        }

        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub async fn submit_language_stats(&self, access_token: &str) -> Result<(), JsValue> {
        use language_utils::profile::UpdateLanguageStatsRequest;

        // Get current stats from the deck (locked cards still count — lockup
        // only hides cards from the review queue)
        let now = js_sys::Date::now();
        let review_info = self.get_review_info_including_locked(now);

        let total_count = review_info.total_count() as i64;

        // Get daily streak information
        let daily_streak = self.get_daily_streak() as i64;
        let daily_streak_expiry = self.stats.daily_streak.as_ref().map(|streak| {
            let tomorrow = streak.last_active_day + chrono::Duration::days(2);
            let expiry = tomorrow
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_local_timezone(self.context.timezone)
                .unwrap();
            expiry.to_rfc3339()
        });

        let xp = self.stats.xp;

        // Get percent_known from the existing method (weighted by word frequency)
        let percent_known = self.get_percent_of_words_known() * 100.0;

        let language = self.context.course.target_language;

        // Get start_time from stats
        let start_time = self.stats.start_time.map(|time| time.to_rfc3339());

        let request = UpdateLanguageStatsRequest {
            language,
            total_count,
            daily_streak,
            daily_streak_expiry,
            xp,
            percent_known,
            start_time,
        };

        let response = crate::utils::hit_ai_server(
            fetch_happen::Method::POST,
            "/language-stats",
            Some(&request),
            Some(&access_token.to_string()),
        )
        .await
        .map_err(|e| JsValue::from_str(&format!("Request error: {e:?}")))?;

        if !response.ok() {
            log::warn!("Failed to update language stats: {}", response.status());
            return Err(JsValue::from_str(&format!(
                "Failed to update language stats: {}",
                response.status()
            )));
        }

        log::info!("Successfully updated language stats");
        Ok(())
    }
}
