use language_utils::{Compensation, Language, language_pack::LanguagePack};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

#[derive(Default)]
struct Registry {
    /// Weak refs to packs owned by `Weapon.language_pack`. The audio fetch
    /// path can't reach the `Weapon`, so it looks packs up here — but holding
    /// `Weak` (not `Arc`) keeps the `Weapon`'s map the sole owner: if a pack
    /// is dropped there, `upgrade()` fails and `lookup` falls through to TTS
    /// rather than serving a stale clip. `register` overwrites on reload.
    packs: BTreeMap<Language, Weak<LanguagePack>>,
    next_actor_index: BTreeMap<(Language, String), usize>,
}

pub fn register(language: Language, pack: &Arc<LanguagePack>) {
    REGISTRY.with(|r| {
        r.borrow_mut().packs.insert(language, Arc::downgrade(pack));
    });
}

pub struct HumanAudio {
    pub bytes: Vec<u8>,
    pub actor_name: String,
    pub compensation: Compensation,
}

/// Return a human-recorded clip for `(language, text)` if any voice actor
/// has a recording for that exact phrase. When multiple actors have a recording,
/// rotates round-robin across successive calls (actors sorted by name).
pub fn lookup(language: Language, text: &str) -> Option<HumanAudio> {
    REGISTRY.with(|r| {
        let mut registry = r.borrow_mut();

        let pack = registry.packs.get(&language)?.upgrade()?;

        let mut actors: Vec<&language_utils::VoiceActor> = pack
            .human_audio
            .iter()
            .filter_map(|(actor, clips)| clips.contains_key(text).then_some(actor))
            .collect();
        if actors.is_empty() {
            return None;
        }
        actors.sort_by(|a, b| a.name.cmp(&b.name));

        let key = (language, text.to_string());
        let counter = registry.next_actor_index.entry(key).or_insert(0);
        let index = *counter % actors.len();
        *counter = counter.wrapping_add(1);

        let actor = actors[index];
        let bytes = pack.human_audio.get(actor)?.get(text)?.bytes.clone();
        Some(HumanAudio {
            bytes,
            actor_name: actor.name.clone(),
            compensation: actor.compensation,
        })
    })
}
