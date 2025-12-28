/// Universal Dependencies morphological features
/// Categorization of features & descriptions come from https://universaldependencies.org/u/feat/
use crate::{Language, PartOfSpeech};
use schemars::JsonSchema;

pub trait FeatureSet {
    fn name() -> &'static str;
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool;
}

/// This feature typically applies to pronouns, pronominal adjectives (determiners), pronominal numerals (quantifiers) and pronominal adverbs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum PronType {}

/// Some languages (especially Slavic) have a complex system of numerals. For example, in the school grammar of Czech, the main part of speech is “numeral”, it includes almost everything where counting is involved and there are various subtypes. It also includes interrogative, relative, indefinite and demonstrative words referring to numbers (words like kolik / how many, tolik / so many, několik / some, a few), so at the same time we may have a non-empty value of PronType. (In English, these words are called quantifiers and they are considered a subgroup of determiners.)
///
/// From the syntactic point of view, some numtypes behave like adjectives and some behave like adverbs. We tag them ADJ and ADV respectively. Thus the NumType feature applies to several different parts of speech:
///
/// NUM: cardinal numerals
/// DET: quantifiers
/// ADJ: definite adjectival, e.g. ordinal numerals
/// ADV: adverbial (e.g. ordinal and multiplicative) numerals, both definite and pronominal
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum NumType {}

/// Boolean feature of pronouns, determiners or adjectives. It tells whether the word is possessive.
///
/// While many tagsets would have “possessive” as one of the various pronoun types, this feature is intentionally separate from PronType, as it is orthogonal to pronominal types. Several of the pronominal types can be optionally possessive, and adjectives can too.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum Poss {
    Yes,
}

/// Boolean feature, typically of pronouns or determiners. It tells whether the word is reflexive, i.e. refers to the subject of its clause.
///
/// While many tagsets would have “reflexive” as one of the various pronoun types, this feature is intentionally separate from PronType. When used with pronouns and determiners, it should be combined with PronType=Prs, regardless whether they really distinguish the Person feature (in some languages they do, in others they do not).
///
/// Note that forms that are canonically reflexive sometimes have other functions in the language, too. The feature Reflex=Yes denotes the word type, not its actual function in context (which can be distinguished by dependency relation types). Hence the feature is not restricted to situations where the word is used truly reflexively.
///
/// For example, reflexive clitics in European languages often have a wide array of possible functions (middle, passive, inchoative, impersonal, or even as a lexical morpheme). Besides that, reflexives in some languages are also used for emphasis (while other languages have separate emphatic pronouns), and in some languages they signal reciprocity (while other languages have separate reciprocal pronouns). Using Reflex=Yes with all of them has the benefit that they can be easily identified (however, if it is possible for the annotators to distinguish contexts where a reflexive pronoun is used reciprocally or emphatically, it is possible to combine Reflex=Yes with PronType=Rcp or PronType=Emp, instead of PronType=Prs).
///
/// Note that while some languages also have reflexive verbs, these are in fact fused verbs with reflexive pronouns, as in Spanish despertarse or Russian проснуться (both meaning “to wake up”). Thus in these cases the fused token will be split to two syntactic words, one of them being a reflexive pronoun. In languages where the reflexive pronoun is not split, it may be more appropriate to mark the verb as the middle Voice than using Reflex=Yes with the verb.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum Reflex {}

/// Clusivity is a feature of first-person plural personal pronouns. As such, it can also be reflected by inflection of verbs, e.g. in Plains Cree (Wolvengrey 2011 p. 66).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum Clusivity {}

/// Gender is usually a lexical feature of nouns and inflectional feature of other parts of speech (pronouns, adjectives, determiners, numerals, verbs) that mark agreement with nouns. In English gender affects only the choice of the personal pronoun (he / she / it) and the feature is usually not encoded in English tagsets.
///
/// See also the related feature of Animacy.
///
/// African languages have an analogous feature of noun classes: there might be separate grammatical categories for flat objects, long thin objects etc.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    JsonSchema,
    Ord,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Gender {
    /// Nouns denoting male persons are masculine. Other nouns may be also grammatically masculine, without any relation to sex.
    Masculine,
    /// Nouns denoting female persons are feminine. Other nouns may be also grammatically feminine, without any relation to sex.
    Feminine,
    /// Some languages have only the masculine/feminine distinction while others also have this third gender for nouns that are neither masculine nor feminine (grammatically).
    Neuter,
    /// Some languages do not distinguish masculine/feminine most of the time but they do distinguish neuter vs. non-neuter (Swedish neutrum / utrum). The non-neuter is called common gender.
    /// Note further that the Com value is not intended for cases where we just cannot derive the gender from the word itself (without seeing the context), while the language actually distinguishes Masc and Fem. For example, in Spanish, nouns distinguish two genders, masculine and feminine, and every noun can be classified as either Masc or Fem. Adjectives are supposed to agree with nouns in gender (and number), which they typically achieve by alternating -o / -a. But then there are adjectives such as grande or feliz that have only one form for both genders. So we cannot tell whether they are masculine or feminine unless we see the context. Yet they are either masculine or feminine (feminine in una ciudad grande, masculine in un puerto grande). Therefore in Spanish we should not tag grande with Gender=Com. Instead, we should either drop the gender feature entirely (suggesting that this word does not inflect for gender) or tag individual instances of grande as either masculine or feminine, depending on context.
    Common,
}

/// Similarly to Gender (and to the African noun classes), animacy is usually a lexical feature of nouns and inflectional feature of other parts of speech (pronouns, adjectives, determiners, numerals, verbs) that mark agreement with nouns. Some languages distinguish only gender, some only animacy, and in some languages both gender and animacy play a role in the grammar. (Some non-UD tagsets then combine the two features into an extended system of genders; however, in UD the two features are annotated separately.)
///
/// Similarly to gender, the values of animacy refer to semantic properties of the noun, but this is only an approximation, referring to the prototypical members of the categroy. There are nouns that are treated as grammatically animate, although semantically the are inanimate.
///
/// The following table is an example of a three-way animacy distinction (human – animate nonhuman – inanimate) in the declension of the masculine determiner który “which” in Polish (boldface forms in the upper and lower rows differ from the middle row):
///
/// gender     sg-nom     sg-gen     sg-dat     sg-acc     sg-ins     sg-loc     pl-nom     pl-gen     pl-dat     pl-acc     pl-ins     pl-loc
/// animate human     który     którego     któremu     którego     którym     którym     którzy     których     którym     których     którymi     których
/// animate non-human     który     którego     któremu     którego     którym     którym     które     których     którym     które     którymi     których
/// inanimate     który     którego     któremu     który     którym     którym     które     których     którym     które     którymi     których
/// In the corresponding paradigm of Czech, only two values are distinguished: masculine animate and masculine inanimate:
///
/// gender     sg-nom     sg-gen     sg-dat     sg-acc     sg-ins     sg-loc     pl-nom     pl-gen     pl-dat     pl-acc     pl-ins     pl-loc
/// animate     který     kterého     kterému     kterého     kterým     kterém     kteří     kterých     kterým     které     kterými     kterých
/// inanimate     který     kterého     kterému     který     kterým     kterém     které     kterých     kterým     které     kterými     kterých
/// More generally: Some languages distinguish animate vs. inanimate (e.g. Czech masculines), some languages distinguish human vs. non-human (e.g. Yuwan, a Ryukyuan language), and others distinguish three values, human vs. non-human animate vs. inanimate (e.g. Polish masculines).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum Animacy {}

/// NounClass is similar to Gender and Animacy because it is to a large part a lexical category of nouns and other parts of speech inflect for it to show agreement (pronouns, adjectives, determiners, numerals, verbs).
///
/// The distinction between gender and noun class is not sharp and is partially conditioned by the traditional terminology of a given language family. In general, the feature is called gender if the number of possible values is relatively low (typically 2-4) and the partition correlates with sex of people and animals. In language families where the number of categories is high (10-20), the feature is usually called noun class. No language family uses both the features.
///
/// In Bantu languages, the noun class also encodes Number; therefore it is a lexical-inflectional feature of nouns. The words should be annotated with the Number feature in addition to NounClass, despite the fact that people who know Bantu could infer the number from the noun class. The lemma of the noun should be its singular form.
///
/// The set of values of this feature is specific for a language family or group. Within the group, it is possible to identify classes that have similar meaning across languages (although some classes may have merged or disappeared in some languages in the group). The value of the NounClass feature consists of a short identifier of the language group (e.g., Bantu), and the number of the class (there is a standardized class numbering system accepted by scholars of the various Bantu languages; similar numbering systems should be created for the other families that have noun classes).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum NounClass {}

/// Number is usually an inflectional feature of nouns and, depending on language, other parts of speech (pronouns, adjectives, determiners, numerals, verbs) that mark agreement with nouns.
///
/// In languages where noun phrases are pluralized using a specific function word (pluralizer), this function word is tagged DET and Number=Plur is its lexical feature.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    JsonSchema,
    Ord,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Number {
    /// Singular number: denotes one person, animal or thing
    Singular,
    /// Plural number: denotes several persons, animals or things
    Plural,
    /// Dual number: denotes two persons, animals or things
    Dual,
    /// Trial number: denotes three persons, animals or things (occurs in pronouns of several Austronesian languages)
    Trial,
    /// Paucal number: denotes "a few" persons, animals or things
    Paucal,
    /// Greater paucal number: denotes "more than several but not many" persons, animals or things (occurs in Sursurunga)
    GreaterPaucal,
    /// Greater plural number: denotes "many, all possible" persons, animals or things
    GreaterPlural,
    /// Inverse number: non-default for that particular noun (occurs in Kiowa)
    Inverse,
    /// Count plural: special plural form used after numerals (e.g., Bulgarian, Russian)
    Count,
    /// Plurale tantum: appears only in plural form even though it denotes one thing (e.g., scissors, pants)
    PluraleTantum,
    /// A special case of singular. It applies to words that use grammatical singular to describe sets of objects, i.e. semantic plural. Although in theory they might be able to form plural, in practice it would be rarely semantically plausible. Sometimes, the plural form exists and means “several sorts of” or “several packages of”.
    Collective,
}

/// Case is usually an inflectional feature of nouns and, depending on language, other parts of speech (pronouns, adjectives, determiners, numerals, verbs) that mark agreement with nouns.
///
/// Case can also be a lexical feature of adpositions and describe the case meaning that the adposition contributes to the nominal in which it appears. (This usage of the feature is typical for languages that do not have case morphology on nouns. For languages that have both adpositions and morphological case, the traditional set of cases is determined by the nominal forms and it does not cover adpositional meanings.) In some non-UD tagsets, case of adpositions is used as a valency feature (saying that the adposition requires its nominal argument to be in that morphological case); however, annotating adposition valency case in UD treebanks would be superfluous because the same case feature can be found at the nominal to which the adposition belongs.
///
/// Case helps specify the role of the noun phrase in the sentence, especially in free-word-order languages. For example, the nominative and accusative cases often distinguish subject and object of the verb, while in fixed-word-order languages these functions would be distinguished merely by the positions of the nouns in the sentence.
///
/// Here on the level of morphosyntactic features we are dealing with case expressed morphologically, i.e. by bound morphemes (affixes). Note that on a higher level case can be understood more broadly as the role, and it can be also expressed by adding an adposition to the noun. What is expressed by affixes in one language can be expressed using adpositions in another language. Cf. the case dependency label.
///
/// Examples
/// [cs] nominative matka “mother”, genitive matky, dative matce, accusative matku, vocative matko, locative matce, instrumental matkou
/// [de] nominative der Mann “the man”, genitive des Mannes, dative dem Mann, accusative den Mann
/// [en] nominative/direct case he, she, accusative/oblique case him, her.
/// The descriptions of the individual case values below include semantic hints about the prototypical meaning of the case. Bear in mind that quite often a case will be used for a meaning that is totally unrelated to the meaning mentioned here. Valency of verbs, adpositions and other words will determine that the noun phrase must be in a particular grammatical case to fill a particular valency slot (semantic role). It is much the same as trying to explain the meaning of prepositions: most people would agree that the central meaning of English in is location in space or time but there are phrases where the meaning is less locational: In God we trust. Say it in English.
///
/// Note that Indian corpora based on the so-called Paninian model use a related feature called vibhakti. It is a merger of the Case feature described here and of various postpositions. Values of the feature are language-dependent because they are copies of the relevant morphemes (either bound morphemes or postpositions). Vibhakti can be mapped on the Case values described here if we know 1. which source values are bound morphemes (postpositions are separate nodes for us) and 2. what is their meaning. For instance, the genitive case (Gen) in Bengali is marked using the suffix -ra (-র), i.e. vib=era. In Hindi, the suffix has been split off the noun and it is now written as a separate word – the postposition kā/kī/ke (का/की/के). Even if the postpositional phrase can be understood as a genitive noun phrase, the noun is not in genitive. Instead, the postposition requires that it takes one of three case forms that are marked directly on the noun: the oblique case (Acc).
///
/// Person is typically feature of personal and possessive pronouns / determiners, and of verbs. On verbs it is in fact an agreement feature that marks the person of the verb’s subject (some languages, e.g. Basque, can also mark person of objects). Person marked on verbs makes it unnecessary to always add a personal pronoun as subject and thus subjects are sometimes dropped (pro-drop languages).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    JsonSchema,
    Ord,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Case {
    /// The base form of the noun, typically used as citation form (lemma). In many languages this is the word form used for subjects of clauses. If the language has only two cases, which are called “direct” and “oblique”, the direct case will be marked Nom.
    Nominative,
    /// Perhaps the second most widely spread morphological case. In many languages this is the word form used for direct objects of verbs. If the language has only two cases, which are called “direct” and “oblique”, the oblique case will be marked Acc.
    Accusative,
    /// Some languages (e.g. Basque) do not use nominative-accusative to distinguish subjects and objects. Instead, they use the contrast of absolutive-ergative.
    ///
    /// The absolutive case marks subject of intransitive verb and direct object of transitive verb.
    Absolutive,
    /// Some languages (e.g. Basque) do not use nominative-accusative to distinguish subjects and objects. Instead, they use the contrast of absolutive-ergative.
    ///
    /// The ergative case marks subject of transitive verb.
    Ergative,
    /// In many languages this is the word form used for indirect objects of verbs.
    Dative,
    /// Prototypical meaning of genitive is that the noun phrase somehow belongs to its governor; it would often be translated by the English preposition of. English has the “saxon genitive” formed by the suffix ‘s; but we will normally not need the feature in English because the suffix gets separated from the noun during tokenization.
    ///
    /// Note that despite considerable semantic overlap, the genitive case is not the same as the feature of possessivity (Poss). Possessivity is a lexical feature, i.e. it applies to lemma and its whole paradigm. Genitive is a feature of just a subset of word forms of the lemma. Semantics of possessivity is much more clearly defined while the genitive (as many other cases) may be required in situations that have nothing to do with possessing. For example, [cs] bez prezidentovy dcery “without the president’s daughter” is a prepositional phrase containing the preposition bez “without”, the possessive adjective prezidentovy “president’s” and the noun dcery “daughter”. The possessive adjective is derived from the noun prezident but it is really an adjective (with separate lemma and paradigm), not just a form of the noun. In addition, both the adjective and the noun are in their genitive forms (the nominative would be prezidentova dcera). There is nothing possessive about this particular occurrence of the genitive. It is there because the preposition bez always requires its argument to be in genitive.
    Genitive,
    /// The vocative case is a special form of noun used to address someone. Thus it predominantly appears with animate nouns (see the feature of Animacy). Nevertheless this is not a grammatical restriction and inanimate things can be addressed as well.
    Vocative,
    /// The role from which the name of the instrumental case is derived is that the noun is used as instrument to do something (as in [cs] psát perem “to write using a pen”). Many other meanings are possible, e.g. in Czech the instrumental is required by the preposition s “with” and thus it includes the meaning expressed in other languages by the comitative case.
    Instrumental,
    /// In Finnish the partitive case expresses indefinite identity and unfinished actions without result.
    Partitive,
    /// The distributive case conveys that something happened to every member of a set, one in a time. Or it may express frequency.
    Distributive,
    /// The essive case expresses a temporary state, often it corresponds to English “as a …” A similar case in Basque is called prolative and it should be tagged Ess too.
    Essive,
    /// The translative case expresses a change of state (“it becomes X”, “it changes to X”). Also used for the phrase “in language X”. In the Szeged Treebank, this case is called factive.
    Translative,
    /// The comitative (also called associative) case corresponds to English “together with …”
    Comitative,
    /// The abessive case (also called caritive or privative) corresponds to the English preposition without.
    Abessive,
    /// Noun in this case is the cause or purpose of something. In Hungarian it also seems to be used frequently with currency (“to buy something for the money”) and it also can mean the goal of something.
    Causative,
    /// The benefactive case corresponds to the English preposition for.
    Benefactive,
    /// The considerative case denotes something that is given in exchange for something else. It is used in Warlpiri (Andrews 2007, p.164).
    Considerative,
    /// The comparative case means “than X”. It marks the standard of comparison and it differs from the comparative Degree, which marks the property being compared. It occurs in Dravidian and Northeast-Caucasian languages.
    Comparative,
    /// The equative case means “X-like”, “similar to X”, “same as X”. It marks the standard of comparison and it differs from the equative Degree, which marks the property being compared. It occurs in Turkish.
    Equative,
    /// The locative case often expresses location in space or time, which gave it its name. As elsewhere, non-locational meanings also exist and they are not rare. Uralic languages have a complex set of fine-grained locational and directional cases (see below) instead of the locative. Even in languages that have locative, some location roles may be expressed using other cases (e.g. because those cases are required by a preposition).
    Locative,
    /// The lative case denotes movement towards/to/into/onto something. Similar case in Basque is called directional allative (Spanish adlativo direccional). However, lative is typically thought of as a union of allative, illative and sublative, while in Basque it is derived from allative, which also exists independently.
    Lative,
    /// The terminative case specifies where something ends in space or time. Similar case in Basque is called terminal allative (Spanish adlativo terminal). While the lative (or directional allative) specifies only the general direction, the terminative (terminal allative) also says that the destination is reached.
    Terminative,
    /// The inessive case expresses location inside of something.
    Inessive,
    /// The illative case expresses direction into something.
    Illative,
    /// The elative case expresses direction out of something.
    Elative,
    /// Distinguished by some scholars in Estonian, not recognized by traditional grammar, exists in the Multext-East Estonian tagset and in the Eesti keele puudepank. It Has the meaning of illative, and some grammars will thus consider the additive just an alternative form of illative. Forms of this case exist only in singular and not For all nouns.
    Additive,
    /// The adessive case expresses location at, on the surface, or near something. The corresponding directional cases are allative (towards something) and ablative (from Something).
    Adessive,
    /// The allative case expresses direction to something (destination is adessive, i.e. at or on that something).
    Allative,
    /// Prototypical meaning: direction from some point. In systems that distinguish different source locatins (e.g. in Uralic languages), this case corresponds to the “adelative”, that is, the source is adessive.
    Ablative,
    /// Used to express location higher than a reference point (atop something or above something). Attested in Nakh-Dagestanian languages and also in Hungarian (while Other Uralic languages express this location with the adessive case, Hungarian has both adessive and superessive).
    Superessive,
    /// The superlative case is used in Nakh-Dagestanian languages to express the destination of movement, originally to the top of something, and, by extension, in other Figurative meanings as well.
    /// Note that Hungarian assigns this meaning to the sublative case, which otherwise indicates that the destination is below (not above) something.
    Superlative,
    /// Used in Hungarian and in Nakh-Dagestanian languages to express the movement from the surface of something (like “moved off the table”).
    ///
    /// Other meanings are possible as well, e.g. “about something”.
    Delative,
    /// Used to express location lower than a reference point (under something or below something). Attested in Nakh-Dagestanian languages.
    Subessive,
    /// The original meaning of the sublative case is movement towards a place under or lower than something, that is, the destination is subessive. It is attested in Nakh-Dagestanian languages. Note however that like many other cases, it is now used in abstract senses that are not apparently connected to the spatial meaning: for Example, in Lezgian it may indicate the cause of something.
    ///
    /// Hungarian uses the sublative label for what would be better categorized as superlative, as it expresses the movement to the surface of something (e.g. “to climb a Tree”), and, by extension, other figurative meanings as well (e.g. “to university”).
    Sublative,
    /// Used to express movement or direction from under something.
    Subelative,
    /// The perlative case denotes movement along something. It is used in Warlpiri (Andrews 2007, p.162). Note that Unimorph mentions the English preposition “along” in Connection with what they call prolative/translative; but we have different definitions of those two cases.
    Perlative,
    /// The temporal case is used to indicate time.
    Temporal,
}

/// Definiteness is typically a feature of nouns, adjectives and articles. Its value distinguishes whether we are talking about something known and concrete, or something general or unknown. It can be marked on definite and indefinite articles, or directly on nouns, adjectives etc. In Arabic, definiteness is also called the "state".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub enum Definite {}

/// Deixis is typically a feature of demonstrative pronouns, determiners, and adverbs. Its value classifies the location of the referred entity with respect to the location of the speaker or of the hearer. The common distinction is distance (proximate vs. remote entities); in some languages, elevation is distinguished as well (e.g., the entity is located higher or lower than the speaker).
///
/// If it is necessary to distinguish the person whose location is the reference point (speaker or hearer), the feature DeixisRef can be used in addition to Deixis. See also the Wolof examples below. DeixisRef is not needed if all deictic expressions in the language are relative to the same person (probably the speaker).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum Deixis {}

/// DeixisRef is a feature of demonstrative pronouns, determiners, and adverbs, accompanying Deixis when necessary. Deixis encodes position of an entity relative to either the speaker or the hearer. If it is necessary to distinguish the person whose location is the reference point (speaker or hearer), DeixisRef is used. DeixisRef is not needed if all deictic expressions in the language are relative to the same person (probably the speaker), or if they do not distinguish the reference point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum DeixisRef {}

/// Degree of comparison is typically an inflectional feature of some adjectives and adverbs. A different flavor of degree is diminutives and augmentatives, which often apply to nouns but are not restricted to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum Degree {}

/// Even though the name of the feature seems to suggest that it is used exclusively with verbs, it is not the case. Some verb forms in some languages actually form a gray zone between verbs and other parts of speech (nouns, adjectives and adverbs). For instance, participles may be either classified as verbs or as adjectives, depending on language and context. In both cases VerbForm=Part may be used to separate them from other verb forms or other types of adjectives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum VerbForm {}

/// Mood is a feature that expresses modality and subclassifies finite verb forms.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    JsonSchema,
    Ord,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Mood {
    /// The indicative can be considered the default mood. A verb in indicative merely states that something happens, has happened or will happen, without adding any attitude of the speaker.
    Indicative,
    /// The speaker uses imperative to order or ask the addressee to do the action of the verb.
    Imperative,
    /// The conditional mood is used to express actions that would have taken place under some circumstances but they actually did not / do not happen. Grammars of some languages may classify conditional as tense (rather than mood) but e.g. in Czech it combines with two different tenses (past and present).
    Conditional,
    /// The action of the verb is possible but not certain. This mood corresponds to the modal verbs can, might, be able to. Used e.g. in Finnish. See also the optative.
    Potential,
    /// The subjunctive mood is used under certain circumstances in subordinate clauses, typically for actions that are subjective or otherwise uncertain. In German, it may be also used to convey the conditional meaning.
    Subjunctive,
    /// The jussive mood expresses the desire that the action happens; it is thus close to both imperative and optative. Unlike in desiderative, it is the speaker, not the subject who wishes that it happens. Used e.g. in Arabic. We also map the Sanskrit injunctive to Mood=Jus.
    Jussive,
    /// Means “in order to”, occurs in Amazonian and Australian languages, such as Arabana.
    Purposive,
    /// The quotative mood is used to express the direct speech of another person.
    Quotative,
    /// Expresses exclamations like “May you have a long life!” or “If only I were rich!” In Turkish it also expresses suggestions. In Sanskrit it may express possibility (cf. the potential mood in other languages).
    Optative,
    /// The desiderative mood corresponds to the modal verb “want to”: “He wants to come.” Used e.g. in Japanese or Turkish.
    Desiderative,
    /// The necessitative mood expresses necessity and corresponds to the modal verbs “must, should, have to”: “He must come.”
    Necessitative,
    /// Verbs in some languages have a special interrogative form that is used in yes-no questions. This is attested, for instance, in the Turkic languages. Celtic languages have it for the copula but not for normal verbs.
    Interrogative,
    /// The irrealis mood denotes an action that is not known to have happened. As such, it is a roof term for a group of more specific moods such as conditional, potential, or desiderative. Some languages do not distinguish these finer shades of meaning but they do distinguish realis (which we tag with the same feature as indicative, Ind) and irrealis.
    Irrealis,
    /// Expresses surprise, irony or doubt. Occurs in Albanian, other Balkan languages, and in Caddo (Native American from Oklahoma).
    Admirative,
}

/// Tense is typically a feature of verbs. It may also occur with other parts of speech (nouns, adjectives, adverbs), depending on whether borderline word forms such as participles are classified as verbs or as the other category.
///
/// Tense is a feature that specifies the time when the action took / takes / will take place, in relation to a reference point. The reference is often the moment of producing the sentence, but it can be also another event in the context. In some languages (e.g. English), some tenses are actually combinations of tense and aspect. In other languages (e.g. Czech), aspect and tense are separate, although not completely independent of each other.
///
/// Note that we are defining features that apply to a single word. If a tense is constructed periphrastically (two or more words, e.g. auxiliary verb indicative + participle of the main verb) and none of the participating words are specific to this tense, then the features will probably not directly reveal the tense. For instance, [en] I had been there is past perfect (pluperfect) tense, formed periphrastically by the simple past tense of the auxiliary to have and the past participle of the main verb to be. The auxiliary will be tagged VerbForm=Fin|Mood=Ind|Tense=Past and the participle will have VerbForm=Part|Tense=Past; none of the two will have Tense=Pqp. On the other hand, Portuguese can form the pluperfect morphologically as just one word, such as estivera, which will thus be tagged VerbForm=Fin|Mood=Ind|Tense=Pqp.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    JsonSchema,
    Ord,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Tense {
    /// The past tense denotes actions that happened before a reference point. In the prototypical case, the reference point is the moment of producing the sentence and the past event happened before the speaker speaks about it. However, Tense=Past is also used to distinguish past participles from other kinds of participles, and past converbs from other kinds of converbs; in these cases, the reference point may itself be in past or future, when compared to the moment of speaking. For instance, the Czech converb spatřivše “having seen” in the sentence spatřivše vojáky, velmi se ulekli “having seen the soldiers, they got very scared” describes an event that is anterior to the event of getting scared. It also happens to be anterior to the moment of speaking, but that fact is not encoded in the converb itself, it is rather a consequence of “getting scared” being in the past tense.
    ///
    /// Among finite forms, the simple past in English is an example of Tense=Past. In German, this is the Präteritum. In Turkish, this is the non-narrative past. In Bulgarian, this is aorist, the aspect-neutral past tense that can be used freely with both imperfective and perfective verbs (see also imperfect).
    Past,
    /// The present tense denotes actions that are in progress (or states that are valid) in a reference point; it may also describe events that usually happen. In the prototypical case, the reference point is the moment of producing the sentence; however, Tense=Pres is also used to distinguish present participles from other kinds of participles, and present converbs from other kinds of converbs. In these cases, the reference point may be in past or future when compared to the moment of speaking. For instance, the English present participle may be used to form a past progressive tense: he was watching TV when I arrived.
    ///
    /// Some languages (e.g. Uralic) only distinguish past vs. non-past morphologically, and then Tense=Pres can be used to represent the non-past form. (In some grammar descriptions, e.g. Turkic or Mongolic, this non-past form may be termed aorist, but note that in other languages the term is actually used for a past tense, as noted above. Therefore the term is better avoided in UD annotation.) Similarly, some Slavic languages (e.g. Czech), although they do distinguish the future tense, nevertheless have a subset of verbs where the morphologically present form has actually a future meaning.
    Present,
    /// The future tense denotes actions that will happen after a reference point; in the prototypical case, the reference point is the moment of producing the sentence.
    Future,
    /// Used in e.g. Bulgarian and Croatian, imperfect is a special case of the past tense. Note that, unfortunately, imperfect tense is not always the same as past tense + imperfective aspect. For instance, in Bulgarian, there is lexical aspect, inherent in verb meaning, and grammatical aspect, which does not necessarily always match the lexical one. In main clauses, imperfective verbs can have imperfect tense and perfective verbs have perfect tense. However, both rules can be violated in embedded clauses.
    Imperfect,
    /// The pluperfect denotes action that happened before another action in past. This value does not apply to English where the pluperfect (past perfect) is constructed analytically. It applies e.g. to Portuguese.
    Pluperfect,
}

/// Aspect is typically a feature of verbs. It may also occur with other parts of speech (nouns, adjectives, adverbs), depending on whether borderline word forms such as gerunds and participles are classified as verbs or as the other category.
///
/// Aspect is a feature that specifies duration of the action in time, whether the action has been completed etc. In some languages (e.g. English), some tenses are actually combinations of tense and aspect. In other languages (e.g. Czech), aspect and tense are separate, although not completely independent of each other.
///
/// In Czech and other Slavic languages, aspect is a lexical feature. Pairs of imperfective and perfective verbs exist and are often morphologically related but the space is highly irregular and the verbs are considered to belong to separate lemmas.
///
/// Since we proceed bottom-up, the current standard covers only a few aspect values found in corpora. See Wikipedia (http://en.wikipedia.org/wiki/Grammatical_aspect) for a long list of other possible aspects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum Aspect {}

/// Voice is typically a feature of verbs. It may also occur with other parts of speech (nouns, adjectives, adverbs), depending on whether borderline word forms such as gerunds and participles are classified as verbs or as the other category.
///
/// For Indo-European speakers, voice means mainly the active-passive distinction. In other languages, other shades of verb meaning are categorized as voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum Voice {}

/// Evidentiality is the morphological marking of a speaker’s source of information (Aikhenvald, 2004). It is sometimes viewed as a category of mood and modality.
///
/// Many different values are attested in the world’s languages. At present we only cover the firsthand vs. non-firsthand distinction, needed in Turkish. It distinguishes there the normal past tense (firsthand, also definite past tense, seen past tense) from the so-called miş-past (non-firsthand, renarrative, indefinite, heard past tense).
///
/// Aikhenvald also distinguishes reported evidentiality, occurring in Estonian and Latvian, among others. We currently use the quotative Mood for this.
///
/// Note: Evident is a new universal feature in UD version 2. It was used as a language-specific feature (under the name Evidentiality) in UD v1 for Turkish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum Evident {}

/// Polarity is typically a feature of verbs, adjectives, sometimes also adverbs and nouns in languages that negate using bound morphemes. In languages that negate using a function word, Polarity is used to mark that function word, unless it is a pro-form already marked with PronType=Neg (see below).
///
/// Positive polarity (affirmativeness) is rarely, if at all, encoded using overt morphology. The feature value Polarity=Pos is usually used to signal that a lemma has negative forms but this particular form is not negative. Using the feature in such cases is somewhat optional for words that can be negated but rarely are. Language-specific documentation should define under which circumstances the positive polarity is annotated.
///
/// In Czech, for instance, all verbs and adjectives can be negated using the prefix ne-.
///
/// In English, verbs are negated using the particle not. English adjectives can be negated with not, or sometimes using prefixes (wise – unwise, probable – improbable), although the use of prefixes is less productive than in Czech. In general, only the most grammatical (as opposed to lexical) forms of negation should receive Polarity=Neg.
///
/// Note that Polarity=Neg is not the same thing as PronType=Neg. For pronouns and other pronominal parts of speech there is no such binary opposition as for verbs and adjectives. (There is no such thing as “affirmative pronoun”.)
///
/// The Polarity feature can be also used to distinguish response interjections yes and no.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub enum Polarity {}

/// Person is typically feature of personal and possessive pronouns / determiners, and of verbs. On verbs it is in fact an agreement feature that marks the person of the verb’s subject (some languages, e.g. Basque, can also mark person of objects). Person marked on verbs makes it unnecessary to always add a personal pronoun as subject and thus subjects are sometimes dropped (pro-drop languages).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    JsonSchema,
    Ord,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Person {
    /// Zero person is for impersonal statements, appears in Finnish as well as in Santa Ana Pueblo Keres. (The construction is distinctive in Finnish but it does not use unique morphology that would necessarily require a feature. However, it is morphologically distinct in Keres (Davis 1964:75): The fourth (zero) person is used “when the subject of the action is obscure, as when the speaker is telling of something that he himself did not observe. It is also used when the subject of the action is inferior to the object, as when an animal is the subject and a human being the object.”
    Zeroth,
    /// In singular, the first person refers just to the speaker / author. In plural, it must include the speaker and one or more additional persons. Some languages (e.g. Taiwanese) distinguish inclusive and exclusive 1st person plural pronouns: the former include the addressee of the utterance (i.e. I + you), the latter exclude them (i.e. I + they).
    First,
    /// In singular, the second person refers to the addressee of the utterance / text. In plural, it may mean several addressees and optionally some third persons too.
    Second,
    /// The third person refers to one or more persons that are neither speakers nor addressees.
    Third,
    /// The fourth person can be understood as a third person argument morphologically distinguished from another third person argument, e.g. in Navajo.
    Fourth,
}

/// Various languages have various means to express politeness or respect; some of the means are morphological. Three to four dimensions of politeness are distinguished in linguistic literature. The Polite feature currently covers (and mixes) two of them; a more elaborate system of feature values may be devised in future versions of UD if needed. The two axes covered are:
///
/// speaker-referent axis (meant to include the addressee when he happens to be the referent)
/// speaker-addressee axis (word forms depend on who is the addressee, although the addressee is not referred to)
/// Changing pronouns and/or person and/or number of the verb forms when respectable persons are addressed in Indo-European languages belongs to the speaker-referent axis because the honorific pronouns are used to refer to the addressee.
///
/// In Czech, formal second person has the same form for singular and plural, and is identical to informal second person plural. This involves both the pronoun and the finite verb but not a participle, which has no special formal form (that is, formal singular is identical to informal singular, not to informal plural).
///
/// In German, Spanish or Hindi, both number and person are changed (informal third person is used as formal second person) and in addition, special pronouns are used that only occur in the formal register ([de] Sie; [es] usted, ustedes; [hi] आप āpa).
///
/// In Japanese, verbs and other words have polite and informal forms but the polite forms are not referring to the addressee (they are not in second person). They are just used because of who the addressee is, even if the topic does not involve the addressee at all. This kind of polite language is called teineigo (丁寧語) and belongs to the speaker-addressee axis. Nevertheless, we currently use the same values for both axes, i.e. Polite=Form can be used for teineigo too. This approach may be refined in future.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    JsonSchema,
    Ord,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum Polite {
    /// Usage varies but if the language distinguishes levels of politeness, then the informal register is usually meant for communication with family members and close friends.
    Informal,
    /// Usage varies but if the language distinguishes levels of politeness, then the polite register is usually meant for communication with strangers and people of higher social status than the one of the speaker.
    Formal,
    /// Usage varies but if the language distinguishes levels of politeness, then the elevated register is usually meant for communication with people of higher social status than the one of the speaker.
    Elev,
    /// Usage varies but if the language distinguishes levels of politeness, then the humble register is usually meant for communication with people of lower social status than the one of the speaker.
    Humb,
}

impl FeatureSet for PronType {
    fn name() -> &'static str {
        "PronType"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Applies to pronouns, pronominal adjectives (determiners), pronominal numerals (quantifiers) and pronominal adverbs
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Pron | PartOfSpeech::Det | PartOfSpeech::Num | PartOfSpeech::Adv
                )
            }
            _ => todo!(),
        }
    }
}

impl FeatureSet for NumType {
    fn name() -> &'static str {
        "NumType"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // NUM: cardinal numerals, DET: quantifiers, ADJ: ordinal numerals, ADV: adverbial numerals
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Num | PartOfSpeech::Det | PartOfSpeech::Adj | PartOfSpeech::Adv
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Poss {
    fn name() -> &'static str {
        "Poss"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Boolean feature of pronouns, determiners or adjectives
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Pron | PartOfSpeech::Det | PartOfSpeech::Adj
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Reflex {
    fn name() -> &'static str {
        "Reflex"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Typically of pronouns or determiners
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(pos, PartOfSpeech::Pron | PartOfSpeech::Det)
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Clusivity {
    fn name() -> &'static str {
        "Clusivity"
    }
    fn applies_to(language: Language, _pos: PartOfSpeech) -> bool {
        // Feature of first-person plural pronouns and can be reflected in verb inflection
        // Not used in our current language set
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => false,
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Gender {
    fn name() -> &'static str {
        "Gender"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        match language {
            Language::English => matches!(pos, PartOfSpeech::Pron),
            Language::French | Language::Spanish => {
                matches!(
                    pos,
                    PartOfSpeech::Noun
                        | PartOfSpeech::Pron
                        | PartOfSpeech::Adj
                        | PartOfSpeech::Det
                        | PartOfSpeech::Num  // Limited to certain numerals
                        | PartOfSpeech::Verb // Only past participles in certain constructions
                )
            }
            Language::German => {
                matches!(
                    pos,
                    PartOfSpeech::Noun
                        | PartOfSpeech::Pron
                        | PartOfSpeech::Adj
                        | PartOfSpeech::Det
                        | PartOfSpeech::Num // Limited (ein/eine/ein)
                )
            }

            Language::Korean => false, // Korean has no grammatical gender
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Animacy {
    fn name() -> &'static str {
        "Animacy"
    }
    fn applies_to(language: Language, _pos: PartOfSpeech) -> bool {
        // Lexical feature of nouns and inflectional feature of other parts of speech
        // Mainly used in Slavic languages (Polish, Czech), not common in our current language set
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => false,
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for NounClass {
    fn name() -> &'static str {
        "NounClass"
    }
    fn applies_to(language: Language, _pos: PartOfSpeech) -> bool {
        // Mainly for Bantu and other African languages, not in our current language set
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => false,
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Number {
    fn name() -> &'static str {
        "Number"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        match language {
            Language::English => {
                matches!(
                    pos,
                    PartOfSpeech::Noun
                        | PartOfSpeech::Pron
                        | PartOfSpeech::Det  // this/these, that/those
                        | PartOfSpeech::Verb
                        | PartOfSpeech::Aux
                )
            }
            Language::French | Language::Spanish => {
                matches!(
                    pos,
                    PartOfSpeech::Noun
                        | PartOfSpeech::Pron
                        | PartOfSpeech::Adj  // Agreement
                        | PartOfSpeech::Det
                        | PartOfSpeech::Verb
                        | PartOfSpeech::Aux
                )
            }
            Language::German => {
                matches!(
                    pos,
                    PartOfSpeech::Noun
                        | PartOfSpeech::Pron
                        | PartOfSpeech::Adj  // Complex with case system
                        | PartOfSpeech::Det
                        | PartOfSpeech::Verb
                        | PartOfSpeech::Aux
                )
            }
            Language::Korean => {
                // Optional plural marking, no verb agreement
                matches!(pos, PartOfSpeech::Noun | PartOfSpeech::Pron)
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Case {
    fn name() -> &'static str {
        "Case"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        match language {
            Language::English => {
                matches!(pos, PartOfSpeech::Pron)
            }
            Language::German => {
                matches!(
                    pos,
                    PartOfSpeech::Noun | PartOfSpeech::Pron | PartOfSpeech::Adj | PartOfSpeech::Det
                )
            }
            Language::Korean => {
                // Case particles (이/가, 을/를, 에, 에서, etc.) are tagged as Part
                matches!(pos, PartOfSpeech::Part)
            }
            Language::French | Language::Spanish => {
                // Limited case in pronouns only
                matches!(pos, PartOfSpeech::Pron)
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Definite {
    fn name() -> &'static str {
        "Definite"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Feature of nouns, adjectives and articles
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Noun | PartOfSpeech::Adj | PartOfSpeech::Det
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Deixis {
    fn name() -> &'static str {
        "Deixis"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Feature of demonstrative pronouns, determiners, and adverbs
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Pron | PartOfSpeech::Det | PartOfSpeech::Adv
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for DeixisRef {
    fn name() -> &'static str {
        "DeixisRef"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Feature of demonstrative pronouns, determiners, and adverbs
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Pron | PartOfSpeech::Det | PartOfSpeech::Adv
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Degree {
    fn name() -> &'static str {
        "Degree"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Inflectional feature of adjectives and adverbs; diminutives/augmentatives apply to nouns
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Adj | PartOfSpeech::Adv | PartOfSpeech::Noun
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for VerbForm {
    fn name() -> &'static str {
        "VerbForm"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Applies to verbs and participles (which may be classified as adjectives)
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Verb | PartOfSpeech::Aux | PartOfSpeech::Adj
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Mood {
    fn name() -> &'static str {
        "Mood"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => {
                matches!(pos, PartOfSpeech::Verb | PartOfSpeech::Aux)
            }
            Language::Chinese => {
                // Chinese lacks verb inflection; mood is expressed through
                // particles and auxiliaries rather than verb forms
                matches!(pos, PartOfSpeech::Aux)
            }
        }
    }
}

impl FeatureSet for Tense {
    fn name() -> &'static str {
        "Tense"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Verb | PartOfSpeech::Aux | PartOfSpeech::Adj // For participles tagged as adjectives
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Aspect {
    fn name() -> &'static str {
        "Aspect"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Feature of verbs; may occur with gerunds and participles classified as other parts of speech
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Verb
                        | PartOfSpeech::Aux
                        | PartOfSpeech::Adj
                        | PartOfSpeech::Noun
                        | PartOfSpeech::Adv
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Voice {
    fn name() -> &'static str {
        "Voice"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Feature of verbs; may occur with gerunds and participles classified as other parts of speech
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Verb
                        | PartOfSpeech::Aux
                        | PartOfSpeech::Adj
                        | PartOfSpeech::Noun
                        | PartOfSpeech::Adv
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Evident {
    fn name() -> &'static str {
        "Evident"
    }
    fn applies_to(language: Language, _pos: PartOfSpeech) -> bool {
        // Evidentiality marking, mainly for Turkish, Estonian, Latvian (not in our current language set)
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => false,
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Polarity {
    fn name() -> &'static str {
        "Polarity"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        // Feature of verbs, adjectives, adverbs, nouns (negation); also function words and interjections (yes/no)
        match language {
            Language::French
            | Language::English
            | Language::Spanish
            | Language::German
            | Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Verb
                        | PartOfSpeech::Aux
                        | PartOfSpeech::Adj
                        | PartOfSpeech::Adv
                        | PartOfSpeech::Noun
                        | PartOfSpeech::Part
                        | PartOfSpeech::Intj
                        | PartOfSpeech::Adp
                )
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Person {
    fn name() -> &'static str {
        "Person"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        match language {
            Language::English => {
                matches!(
                    pos,
                    PartOfSpeech::Pron
                        | PartOfSpeech::Det  // my/your/their
                        | PartOfSpeech::Verb // limited: -s for 3sg
                        | PartOfSpeech::Aux // am/is/are, have/has
                )
            }
            Language::French | Language::Spanish | Language::German => {
                matches!(
                    pos,
                    PartOfSpeech::Pron
                        | PartOfSpeech::Det  // Possessive determiners
                        | PartOfSpeech::Verb
                        | PartOfSpeech::Aux
                )
            }
            Language::Korean => {
                // Korean pronouns exist but verbs don't inflect for person
                matches!(pos, PartOfSpeech::Pron)
            }
            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

impl FeatureSet for Polite {
    fn name() -> &'static str {
        "Polite"
    }
    fn applies_to(language: Language, pos: PartOfSpeech) -> bool {
        match language {
            // T-V distinction languages (tu/vous, du/Sie, tú/usted)
            Language::German | Language::Spanish | Language::French => {
                matches!(
                    pos,
                    PartOfSpeech::Pron | PartOfSpeech::Det | PartOfSpeech::Verb | PartOfSpeech::Aux
                )
            }
            // Honorific system with verb marking
            Language::Korean => {
                matches!(
                    pos,
                    PartOfSpeech::Verb | PartOfSpeech::Aux | PartOfSpeech::Pron
                )
            }
            // English lacks morphological politeness
            Language::English => false,

            Language::Chinese
            | Language::Japanese
            | Language::Russian
            | Language::Portuguese
            | Language::Italian => todo!(),
        }
    }
}

// Just gender, politeness, tense and person for now
#[derive(
    Clone,
    Debug,
    serde::Deserialize,
    schemars::JsonSchema,
    serde::Serialize,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    tsify::Tsify,
    Default,
)]
#[rkyv(compare(PartialEq), derive(Debug))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Morphology {
    pub gender: Option<Gender>,
    pub number: Option<Number>,
    pub politeness: Option<Polite>,
    pub tense: Option<Tense>,
    pub person: Option<Person>,
    pub case: Option<Case>,
    pub mood: Option<Mood>,
}

/// Represents a grammatical prefix for a word (like articles in French)
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    schemars::JsonSchema,
    serde::Serialize,
    serde::Deserialize,
    tsify::Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WordPrefix {
    /// The prefix text (e.g., "le", "la", "les", "l'")
    pub prefix: String,
    /// The separator between prefix and word (e.g., " " for "le chat", "" for "l'arbre")
    pub separator: String,
}

impl Morphology {
    /// Generates a grammatical prefix for a word based on its morphology and part of speech.
    ///
    /// Returns the appropriate prefix for:
    /// - **Nouns**: Definite articles
    ///   - French: "le", "la", "les", or "l'" (with elision)
    ///   - Spanish: "el", "la", "los", "las"
    ///   - German: "der", "die", "das", etc. (case-aware)
    /// - **Verbs**: Subject pronouns for conjugated forms, or "to" for English infinitives
    ///   - French: "je", "tu", "il", "nous", "vous", "ils"
    ///   - Spanish: "yo", "tú", "él", "nosotros", "vosotros", "ellos" (politeness-aware)
    ///   - German: "ich", "du", "er", "wir", "ihr", "sie" (politeness-aware)
    ///   - English: "I", "you", "he", "we", "they", or "to" for infinitives
    ///
    /// # Arguments
    /// * `word` - The word to generate a prefix for (used to check for elision in French)
    /// * `pos` - The part of speech
    /// * `language` - The language
    ///
    /// # Returns
    /// * `Some(WordPrefix)` if a prefix is appropriate for this word
    /// * `None` if no prefix should be used
    pub fn get_prefix(
        &self,
        word: &str,
        pos: PartOfSpeech,
        language: Language,
    ) -> Option<WordPrefix> {
        match (language, pos) {
            (Language::French, PartOfSpeech::Noun) => self.get_french_noun_prefix(word),
            (Language::French, PartOfSpeech::Verb | PartOfSpeech::Aux) => {
                self.get_french_verb_prefix()
            }
            (Language::French, _) => None,
            (Language::Spanish, PartOfSpeech::Noun) => self.get_spanish_noun_prefix(),
            (Language::Spanish, PartOfSpeech::Verb | PartOfSpeech::Aux) => {
                self.get_spanish_verb_prefix()
            }
            (Language::Spanish, _) => None,
            (Language::German, PartOfSpeech::Noun) => self.get_german_noun_prefix(),
            (Language::German, PartOfSpeech::Verb | PartOfSpeech::Aux) => {
                self.get_german_verb_prefix()
            }
            (Language::German, _) => None,
            (Language::English, PartOfSpeech::Verb | PartOfSpeech::Aux) => {
                self.get_english_verb_prefix()
            }
            (Language::Korean | Language::English, _) => None,

            (
                Language::Chinese
                | Language::Japanese
                | Language::Russian
                | Language::Portuguese
                | Language::Italian,
                _,
            ) => todo!(),
        }
    }

    fn get_french_noun_prefix(&self, word: &str) -> Option<WordPrefix> {
        // Check if the word starts with a vowel sound for elision
        // In French, elision occurs before vowels and silent h
        let starts_with_vowel = word
            .chars()
            .next()
            .map(|c| {
                matches!(
                    c.to_ascii_lowercase(),
                    'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'h'
                )
            })
            .unwrap_or(false);

        if starts_with_vowel {
            // Use elided form regardless of gender/number
            Some(WordPrefix {
                prefix: "l'".to_string(),
                separator: "".to_string(),
            })
        } else {
            // Plural takes precedence over gender
            if let Some(Number::Plural) = self.number {
                Some(WordPrefix {
                    prefix: "les".to_string(),
                    separator: " ".to_string(),
                })
            } else {
                // Singular: use gender to determine article
                let prefix = match self.gender {
                    Some(Gender::Feminine) => "la",
                    Some(Gender::Masculine) | None => "le",
                    _ => "le", // Default to masculine for neuter/common
                };
                Some(WordPrefix {
                    prefix: prefix.to_string(),
                    separator: " ".to_string(),
                })
            }
        }
    }

    fn get_spanish_noun_prefix(&self) -> Option<WordPrefix> {
        let prefix = match self.number {
            Some(Number::Plural) => {
                // Plural articles
                match self.gender {
                    Some(Gender::Feminine) => "las",
                    Some(Gender::Masculine) | None => "los",
                    _ => "los", // Default to masculine
                }
            }
            _ => {
                // Singular articles (including None, which defaults to singular)
                match self.gender {
                    Some(Gender::Feminine) => "la",
                    Some(Gender::Masculine) | None => "el",
                    _ => "el", // Default to masculine
                }
            }
        };

        Some(WordPrefix {
            prefix: prefix.to_string(),
            separator: " ".to_string(),
        })
    }

    fn get_german_noun_prefix(&self) -> Option<WordPrefix> {
        // German articles depend on gender, number, and case
        // Default to nominative case if not specified
        let case = self.case.unwrap_or(Case::Nominative);

        let prefix = match self.number {
            Some(Number::Plural) => {
                // Plural articles (same for all genders)
                match case {
                    Case::Nominative | Case::Accusative => "die",
                    Case::Dative => "den",
                    Case::Genitive => "der",
                    _ => "die", // Default to nominative
                }
            }
            _ => {
                // Singular articles
                match (case, self.gender) {
                    // Nominative case
                    (Case::Nominative, Some(Gender::Masculine)) => "der",
                    (Case::Nominative, Some(Gender::Feminine)) => "die",
                    (Case::Nominative, Some(Gender::Neuter)) => "das",
                    (Case::Nominative, _) => return None,

                    // Accusative case
                    (Case::Accusative, Some(Gender::Masculine)) => "den",
                    (Case::Accusative, Some(Gender::Feminine)) => "die",
                    (Case::Accusative, Some(Gender::Neuter)) => "das",
                    (Case::Accusative, _) => return None,

                    // Dative case
                    (Case::Dative, Some(Gender::Masculine)) => "dem",
                    (Case::Dative, Some(Gender::Feminine)) => "der",
                    (Case::Dative, Some(Gender::Neuter)) => "dem",
                    (Case::Dative, _) => return None,

                    // Genitive case
                    (Case::Genitive, Some(Gender::Masculine)) => "des",
                    (Case::Genitive, Some(Gender::Feminine)) => "der",
                    (Case::Genitive, Some(Gender::Neuter)) => "des",
                    (Case::Genitive, _) => return None,

                    // For other cases, default to nominative
                    (_, Some(Gender::Masculine)) => "der",
                    (_, Some(Gender::Feminine)) => "die",
                    (_, Some(Gender::Neuter)) => "das",
                    (_, _) => return None,
                }
            }
        };

        Some(WordPrefix {
            prefix: prefix.to_string(),
            separator: " ".to_string(),
        })
    }

    fn get_french_verb_prefix(&self) -> Option<WordPrefix> {
        // For conjugated verbs, add subject pronouns based on person/number
        // For infinitives (no person), no prefix
        let person = self.person?;
        let number = self.number.unwrap_or(Number::Singular);
        let politeness = self.politeness;

        let prefix = match (person, number, politeness) {
            (Person::First, Number::Singular, _) => "je",
            (Person::First, Number::Plural, _) => "nous",
            (Person::Second, Number::Singular, Some(Polite::Formal)) => "vous",
            (Person::Second, Number::Singular, _) => "tu",
            (Person::Second, Number::Plural, _) => "vous",
            (Person::Third, Number::Singular, _) => "il",
            (Person::Third, Number::Plural, _) => "ils",
            _ => return None,
        };

        Some(WordPrefix {
            prefix: prefix.to_string(),
            separator: " ".to_string(),
        })
    }

    fn get_spanish_verb_prefix(&self) -> Option<WordPrefix> {
        // For conjugated verbs, add subject pronouns based on person/number
        // For infinitives (no person), no prefix
        let person = self.person?;
        let number = self.number.unwrap_or(Number::Singular);
        let politeness = self.politeness;

        let prefix = match (person, number, politeness) {
            (Person::First, Number::Singular, _) => "yo",
            (Person::First, Number::Plural, _) => "nosotros",
            (Person::Second, Number::Singular, Some(Polite::Formal)) => "usted",
            (Person::Second, Number::Singular, _) => "tú",
            (Person::Second, Number::Plural, Some(Polite::Formal)) => "ustedes",
            (Person::Second, Number::Plural, _) => "vosotros",
            (Person::Third, Number::Singular, _) => "él",
            (Person::Third, Number::Plural, _) => "ellos",
            _ => return None,
        };

        Some(WordPrefix {
            prefix: prefix.to_string(),
            separator: " ".to_string(),
        })
    }

    fn get_german_verb_prefix(&self) -> Option<WordPrefix> {
        // For conjugated verbs, add subject pronouns based on person/number
        // For infinitives (no person), no prefix
        let person = self.person?;
        let number = self.number.unwrap_or(Number::Singular);
        let politeness = self.politeness;

        let prefix = match (person, number, politeness) {
            (Person::First, Number::Singular, _) => "ich",
            (Person::First, Number::Plural, _) => "wir",
            (Person::Second, Number::Singular, Some(Polite::Formal)) => "Sie",
            (Person::Second, Number::Singular, _) => "du",
            (Person::Second, Number::Plural, Some(Polite::Formal)) => "Sie",
            (Person::Second, Number::Plural, _) => "ihr",
            (Person::Third, Number::Singular, _) => "er",
            (Person::Third, Number::Plural, _) => "sie",
            _ => return None,
        };

        Some(WordPrefix {
            prefix: prefix.to_string(),
            separator: " ".to_string(),
        })
    }

    fn get_english_verb_prefix(&self) -> Option<WordPrefix> {
        // For infinitives (no person), add "to"
        // For conjugated verbs, add subject pronouns
        if let Some(person) = self.person {
            let number = self.number.unwrap_or(Number::Singular);

            let prefix = match (person, number) {
                (Person::First, Number::Singular) => "I",
                (Person::First, Number::Plural) => "we",
                (Person::Second, _) => "you",
                (Person::Third, Number::Singular) => "he",
                (Person::Third, Number::Plural) => "they",
                _ => return None,
            };

            Some(WordPrefix {
                prefix: prefix.to_string(),
                separator: " ".to_string(),
            })
        } else {
            // Infinitive: add "to"
            Some(WordPrefix {
                prefix: "to".to_string(),
                separator: " ".to_string(),
            })
        }
    }
}
