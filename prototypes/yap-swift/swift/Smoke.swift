import Foundation

private func check(_ condition: Bool, file: StaticString = #file, line: UInt = #line) { precondition(condition, file: file, line: line) }

@main struct YapSmoke {
    @MainActor static func main() async throws {
        // Shared language copy crosses the real native bridge, including Unicode,
        // optional script qualifiers, and accent keyboard arrays.
        let french = get_language_metadata(language: .French)
        check(french.native_name == "Français" && french.flag == "🇫🇷")
        check(french.i_speak == "Je parle français" && french.lets_go == "Allons-y !")
        check(french.accented_characters.contains("œ") && french.script == nil)
        let simplified = get_language_metadata(language: .ChineseSimplified)
        let traditional = get_language_metadata(language: .ChineseTraditional)
        check(simplified.script == "Hans" && traditional.script == "Hant")
        check(simplified.iso_code != traditional.iso_code && simplified.iso6391 == traditional.iso6391)
        check(traditional.native_name == "繁體中文" && traditional.i_speak == "我說中文")
        check(get_language_metadata(language: .English).character_type == nil)
        try YapHost.initialize()
        do {
            try YapHost.initialize()
            fatalError("runtime replacement should be rejected")
        } catch let error as BridgeError {
            check(error.description.contains("already"))
        }
        let course = Course(native_language: .English, target_language: .French)
        var syncStreams: [String] = []
        var syncKeys: [ListenerKey] = []
        let weapon = try await Weapon.create(user_id: nil) { key, stream in
            check(Thread.isMainThread)
            syncKeys.append(key)
            syncStreams.append(stream)
        }
        let device = weapon.device_id
        check(!device.isEmpty)
        var notifications = 0
        let listener = weapon.subscribe_to_stream(stream_id: "reviews") { [weak weapon] in
            check(Thread.isMainThread)
            check(weapon!.device_id == device) // reenter the real Weapon
            notifications += 1
        }
        weapon.request_reviews()
        check(notifications > 0 && syncStreams.contains("reviews") && !syncKeys.isEmpty)
        let modifier = syncKeys.removeFirst()
        try await weapon.sync(stream_id: "reviews", access_token: nil, attempt_supabase: false, modifier: modifier, upload: false)
        syncKeys.removeAll()
        try await weapon.load_from_local_storage(stream_id: "reviews")
        let beforeInvalidEvents = weapon.num_events
        for (invalidEvent, message) in [("{", "EOF"), ("{}", "missing field")] {
            do {
                try weapon.add_remote_event(device_id: "invalid-fixture", stream_id: "reviews", event: invalidEvent)
                fatalError("expected a JSON error")
            } catch let error as BridgeError { check(error.description.contains(message)) }
        }
        check(weapon.num_events == beforeInvalidEvents)
        do {
            _ = try await weapon.get_deck_state(course: course, utc_offset_seconds: 0)
            fatalError("expected a missing-pack error")
        } catch let error as BridgeError {
            check(error.description.contains("not loaded"))
        }
        let unsupported = Course(native_language: .English, target_language: .English)
        do {
            try await weapon.load_language_pack(course: unsupported, on_progress: nil)
            fatalError("expected unsupported course")
        } catch LanguageDataError.UnsupportedCourse(let actual) { check(actual == unsupported) }
        var progressCalls = 0
        try await weapon.load_language_pack_core(course: course) { message, percent in
            check(!message.isEmpty && percent >= 0 && percent <= 100)
            check(Thread.isMainThread)
            progressCalls += 1
        }
        check(!weapon.is_language_pack_fully_loaded(course: course) && progressCalls > 0)
        _ = try await weapon.get_deck_state(course: course, utc_offset_seconds: 0)
        try await weapon.load_language_pack(course: course, on_progress: nil)
        check(weapon.is_language_pack_fully_loaded(course: course))
        let original = try await weapon.get_deck_state(course: course, utc_offset_seconds: 0)
        // A stale clip would be evicted by cleanup if cancellation incorrectly fell through.
        let audioDirectory = URL(fileURLWithPath: String(cString: getenv("YAP_DATA_DIR")!)).appendingPathComponent("audio")
        try FileManager.default.createDirectory(at: audioDirectory, withIntermediateDirectories: true)
        let sentinel = audioDirectory.appendingPathComponent("cancel-preserves.mp3")
        try Data([1, 2, 3]).write(to: sentinel)
        try Data("{\"cancel-preserves.mp3\":1}".utf8).write(to: audioDirectory.appendingPathComponent("last_used.json"))
        let aborted = AbortController()
        aborted.abort()
        await original.cache_challenge_audio(banned_challenge_types: [], access_token: nil, abort_signal: aborted.signal())
        let prefetch = Task { @MainActor in
            await original.cache_challenge_audio(banned_challenge_types: [.Text, .Listening, .Speaking], access_token: nil)
        }
        prefetch.cancel()
        await prefetch.value
        let background = Task { @MainActor in
            await original.cache_challenge_audio(banned_challenge_types: [.Text, .Listening, .Speaking], access_token: nil)
        }
        try await Task.sleep(for: .milliseconds(20))
        background.cancel()
        await background.value
        check(FileManager.default.fileExists(atPath: sentinel.path))
        let summary = original.get_today_summary()
        check(summary.reviews == 0 && !summary.day_of_week.isEmpty)
        check(original.get_daily_review_target() == 600)
        let root = URL(fileURLWithPath: CommandLine.arguments[1])
        let event = try String(contentsOf: root.appendingPathComponent("event-0.json"), encoding: .utf8)
        let before = notifications
        try weapon.add_remote_event(device_id: "swift-prototype-fixture", stream_id: "reviews", event: event)
        check(notifications == before + 1)
        let changed = try await weapon.get_deck_state(course: course, utc_offset_seconds: 0)
        check(changed.get_daily_review_target() == 1200)
        check(original.get_daily_review_target() == 600) // independent Deck snapshots
        weapon.unsubscribe(key: listener)
        let after = notifications
        let secondEvent = try String(contentsOf: root.appendingPathComponent("event-1.json"), encoding: .utf8)
        try weapon.add_remote_event(device_id: "swift-prototype-fixture", stream_id: "reviews", event: secondEvent)
        check(notifications == after)
        let latest = try await weapon.get_deck_state(course: course, utc_offset_seconds: 0)
        check(latest.get_daily_review_target() == 300)
        // The full Weapon impl is exported: Swift creates typed events directly.
        check(weapon.user_id == nil)
        check(weapon.num_events >= 2)
        let reviewCount = weapon.get_stream_num_events(stream_id: "reviews")!
        let goal = DeckEvent.Language(LanguageEvent(target_language: .French, native_language: .English,
            content: .SetDailyReviewTarget(daily_review_target: .Intense)))
        weapon.add_deck_event(event: goal)
        // Exercise an explicitly later timestamp through the same event API.
        let future = Date().timeIntervalSince1970 * 1000 + 10_000
        weapon.add_deck_event_at(event: goal, timestamp_ms: future)
        check(weapon.get_stream_num_events(stream_id: "reviews")! == reviewCount + 2)
        weapon.request_deck_selection()
        try await weapon.load_from_local_storage(stream_id: "deck_selection")
        try weapon.add_deck_selection_event(event: .SelectBothLanguages(native: .English, target: .French))
        let selection = weapon.get_deck_selection_state()!
        check(selection.native_language == .English && selection.target_language == .French)
        let sync = weapon.get_sync_state(target: .Supabase)
        check(sync.remote_clock.isEmpty && sync.last_sync_error == nil)
        check(weapon.num_events_on_remote_as_of_last_sync(target: .Supabase) == 0)
        let earliest = weapon.get_timestamp_of_earliest_unsynced_event(target: .Supabase)!
        check(earliest.timestamp.seconds > 0 && earliest.timestamp.nanoseconds < 1_000_000_000)
        // Offline sync runs the same load/save pipeline against native storage.
        try await weapon.sync(stream_id: "reviews", access_token: nil, attempt_supabase: false, modifier: nil, upload: false)
        try await weapon.sync(stream_id: "deck_selection", access_token: nil, attempt_supabase: false, modifier: nil, upload: false)
        try await weapon.sync_with_supabase(access_token: "unused-offline", modifier: nil, upload: false)
        let reopened = try await Weapon.create(user_id: nil) { _, _ in }
        check(reopened.device_id == device)
        reopened.request_reviews()
        reopened.request_deck_selection()
        try await reopened.load_from_local_storage(stream_id: "reviews")
        try await reopened.load_from_local_storage(stream_id: "deck_selection")
        check(reopened.get_stream_num_events(stream_id: "reviews") == reviewCount + 2)
        check(reopened.get_deck_selection_state() == selection)
        // Disable fixture downloads; reopening must use the shared chunk cache.
        let backend = String(cString: getenv("YAP_AI_BACKEND_URL")!)
        var offlineRequest = URLRequest(url: URL(string: "\(backend)/__offline")!)
        offlineRequest.httpMethod = "POST"
        let (_, response) = try await URLSession.shared.data(for: offlineRequest)
        check((response as? HTTPURLResponse)?.statusCode == 204)
        try await reopened.cache_language_pack(course: course)
        let persisted = try await reopened.get_deck_state(course: course, utc_offset_seconds: 0)
        check(persisted.get_daily_review_target() == 1200)
        do {
            _ = try await weapon.get_deck_state(course: course, utc_offset_seconds: 100_000)
            fatalError("expected invalid timezone error")
        } catch let error as BridgeError { check(error.description.contains("timezone")) }
        // Exercise the full Deck interface through generated objects, values, and getters.
        check(persisted.get_target_language() == .French)
        check(persisted.get_all_cards_summary().isEmpty && persisted.locked_count() == 0)
        check(persisted.get_lockup_offer(banned_challenge_types: [], timestamp_ms: future) == nil)
        check(persisted.get_release_offer(timestamp_ms: future) == nil)
        check(persisted.get_current_tier().tier > 0)
        check(persisted.get_current_week_progress().count == 7)
        check(persisted.get_movie_poster(movie_id: "missing-smoke-movie") == nil)
        let ready = persisted.get_no_cards_ready_info(banned_challenge_types: [.Listening, .Speaking], sentence_list: nil)
        check(ready.smart_add_count > 0 && ready.smart_add_event != nil)
        let add = persisted.get_manual_add_option(card_type: .TargetLanguage, sentence_list: nil)
        check(add.count > 0 && add.event != nil)
        reopened.add_deck_event(event: add.event!)
        let withCards = try await reopened.get_deck_state(course: course, utc_offset_seconds: 0)
        let cards = withCards.get_all_cards_summary()
        check(cards.count == Int(add.count) && !cards[0].card_text.isEmpty)
        check(cards[0].due_timestamp_ms.isFinite && !cards[0].state.isEmpty)
        let review = withCards.get_review_info(banned_challenge_types: [.Listening, .Speaking], timestamp_ms: future)
        check(review.total_count == UInt64(cards.count) && review.due_count > 0)
        check(review.get_next_challenge(deck: withCards) != nil)
        let reviewed = withCards.review_card(reviewed: cards[0].card_indicator, rating: .Good)
        check(reviewed != nil)
        reopened.add_deck_event(event: reviewed!)
        let afterReview = try await reopened.get_deck_state(course: course, utc_offset_seconds: 0)
        check(afterReview.get_total_reviews() == withCards.get_total_reviews() + 1)
        print("PASS: real Weapon factory, native filesystem, stable device ID, callbacks/reentrancy/unsubscribe, real French pack, returned Deck objects, TodaySummary, typed event inputs, optional counts, sync state/timestamps, and native persistence/reopen")
    }
}
