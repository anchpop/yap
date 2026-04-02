use language_utils::{Language, PartOfSpeechTag};

/// A possible meaning of a polysemous word: (lemma, POS)
pub type Meaning = (&'static str, PartOfSpeechTag);

/// A polysemous word entry: (surface_form, meanings)
pub type PolysemousWord = (&'static str, &'static [Meaning]);

/// Get the polysemous word list for a given language.
/// These are surface forms that have multiple distinct meanings (different lemma or POS),
/// excluding pure AUX/VERB same-lemma distinctions and pure DET/PRON distinctions
/// which are already handled by dedicated classifier logic.
pub fn polysemous_words(language: Language) -> &'static [PolysemousWord] {
    match language {
        Language::French => FRENCH,
        Language::Spanish => SPANISH,
        Language::German => GERMAN,
        Language::Italian => ITALIAN,
        Language::Portuguese => PORTUGUESE,
        Language::Chinese => CHINESE,
        Language::Japanese => JAPANESE,
        Language::Hindi => HINDI,
        _ => &[],
    }
}

use PartOfSpeechTag::*;

const FRENCH: &[PolysemousWord] = &[
    ("pas", &[("pas", Adv), ("pas", Noun)]),
    ("un", &[("un", Det), ("un", Pron), ("un", Num)]),
    ("une", &[("un", Det), ("un", Pron), ("un", Num)]),
    ("a", &[("avoir", Aux), ("avoir", Verb), ("a", Adp)]),
    (
        "bien",
        &[("bien", Adv), ("bien", Intj), ("bien", Noun), ("bien", Adj)],
    ),
    ("tout", &[("tout", Pron), ("tout", Det), ("tout", Adv)]),
    ("son", &[("son", Det), ("son", Noun)]),
    ("non", &[("non", Intj), ("non", Adv)]),
    ("si", &[("si", Sconj), ("si", Adv), ("si", Intj)]),
    ("fait", &[("faire", Verb), ("fait", Noun), ("fait", Adj)]),
    ("quoi", &[("quoi", Pron), ("quoi", Intj)]),
    ("tous", &[("tout", Pron), ("tout", Det), ("tout", Adv)]),
    ("juste", &[("juste", Adv), ("juste", Adj)]),
    ("demain", &[("demain", Adv), ("demain", Noun)]),
    ("bon", &[("bon", Adj), ("bon", Intj), ("bon", Adv)]),
    ("après", &[("après", Adp), ("après", Adv)]),
    ("bonjour", &[("bonjour", Intj), ("bonjour", Noun)]),
    ("mieux", &[("mieux", Adv), ("mieux", Noun)]),
    ("même", &[("même", Adv), ("même", Adj)]),
    ("voilà", &[("voilà", Verb), ("voilà", Intj)]),
    ("avant", &[("avant", Adp), ("avant", Adv)]),
    ("mal", &[("mal", Adv), ("mal", Noun)]),
    ("petit", &[("petit", Adj), ("petit", Noun)]),
    ("voici", &[("voici", Verb), ("voici", Adv), ("voici", Intj)]),
    ("personne", &[("personne", Pron), ("personne", Noun)]),
    ("porte", &[("porter", Verb), ("porte", Noun)]),
    ("tiens", &[("tenir", Verb), ("tiens", Intj)]),
    ("vieux", &[("vieux", Adj), ("vieux", Noun)]),
    ("toutes", &[("tout", Det), ("tout", Pron), ("tout", Adv)]),
    ("toute", &[("tout", Det), ("tout", Adv)]),
    ("fort", &[("fort", Adv), ("fort", Adj)]),
    ("derrière", &[("derrière", Adp), ("derrière", Adv)]),
    ("entre", &[("entre", Adp), ("entrer", Verb)]),
    ("devant", &[("devant", Adp), ("devant", Adv)]),
    ("français", &[("français", Noun), ("français", Adj)]),
    ("compte", &[("compter", Verb), ("compte", Noun)]),
    (
        "pouvoir",
        &[("pouvoir", Aux), ("pouvoir", Verb), ("pouvoir", Noun)],
    ),
    ("garde", &[("garder", Verb), ("garde", Noun)]),
    ("petite", &[("petit", Adj), ("petite", Noun)]),
    ("attention", &[("attention", Intj), ("attention", Noun)]),
    ("super", &[("super", Adj), ("super", Adv), ("super", Intj)]),
    ("montre", &[("montrer", Verb), ("montre", Noun)]),
    ("demande", &[("demander", Verb), ("demande", Noun)]),
    ("sens", &[("sens", Noun), ("sentir", Verb)]),
    ("mort", &[("mort", Adj), ("mort", Noun), ("mourir", Verb)]),
    ("rouge", &[("rouge", Adj), ("rouge", Noun)]),
    ("été", &[("être", Aux), ("être", Verb), ("été", Noun)]),
    ("part", &[("part", Noun), ("partir", Verb)]),
    ("aide", &[("aider", Verb), ("aide", Noun)]),
    ("voyage", &[("voyage", Noun), ("voyager", Verb)]),
    ("vit", &[("vivre", Verb), ("voir", Verb)]),
    ("fini", &[("finir", Verb), ("fini", Adj)]),
    (
        "calme",
        &[("calme", Adj), ("calme", Noun), ("calmer", Verb)],
    ),
    ("suis", &[("être", Aux), ("suivre", Verb)]),
    ("dîner", &[("dîner", Noun), ("dîner", Verb)]),
    ("anglais", &[("anglais", Noun), ("anglais", Adj)]),
    ("possible", &[("possible", Adj), ("possible", Noun)]),
    ("cours", &[("cours", Noun), ("courir", Verb)]),
    ("pardon", &[("pardon", Intj), ("pardon", Noun)]),
    ("merde", &[("merde", Intj), ("merde", Noun)]),
    ("bienvenue", &[("bienvenue", Intj), ("bienvenue", Noun)]),
    ("téléphone", &[("téléphone", Noun), ("téléphoner", Verb)]),
    ("occupé", &[("occupé", Adj), ("occuper", Verb)]),
    ("debout", &[("debout", Adv), ("debout", Intj)]),
];

const SPANISH: &[PolysemousWord] = &[
    ("no", &[("no", Adv), ("no", Intj), ("no", Part)]),
    (
        "que",
        &[("que", Sconj), ("que", Pron), ("qué", Pron), ("qué", Det)],
    ),
    ("para", &[("para", Adp), ("parar", Verb)]),
    ("más", &[("más", Adv), ("más", Det)]),
    (
        "como",
        &[
            ("como", Adp),
            ("como", Sconj),
            ("comer", Verb),
            ("como", Cconj),
            ("cómo", Adv),
        ],
    ),
    ("bien", &[("bien", Adv), ("bien", Intj)]),
    ("sí", &[("sí", Intj), ("sí", Adv)]),
    ("solo", &[("solo", Adv), ("solo", Adj)]),
    ("este", &[("este", Det), ("este", Pron), ("este", Noun)]),
    ("qué", &[("qué", Pron), ("qué", Det), ("qué", Adv)]),
    ("bueno", &[("bueno", Intj), ("bueno", Adj)]),
    ("vamos", &[("ir", Verb), ("vamos", Intj), ("ir", Aux)]),
    ("mañana", &[("mañana", Adv), ("mañana", Noun)]),
    ("mejor", &[("mejor", Adv), ("mejor", Adj)]),
    ("sobre", &[("sobre", Adp), ("sobre", Noun)]),
    ("uno", &[("uno", Pron), ("uno", Num)]),
    ("demasiado", &[("demasiado", Adv), ("demasiado", Det)]),
    ("trabajo", &[("trabajo", Noun), ("trabajar", Verb)]),
    ("sé", &[("saber", Verb), ("ser", Aux), ("saber", Aux)]),
    ("juntos", &[("junto", Adj), ("junto", Adv)]),
    ("mal", &[("mal", Adv), ("malo", Adj)]),
    ("pues", &[("pues", Cconj), ("pues", Intj)]),
    ("entre", &[("entre", Adp), ("entrar", Verb)]),
    ("tanto", &[("tanto", Adv), ("tanto", Det)]),
    ("viejo", &[("viejo", Adj), ("viejo", Noun)]),
    ("fuera", &[("fuera", Adv), ("ser", Aux), ("ir", Verb)]),
    ("vino", &[("venir", Verb), ("vino", Noun)]),
    ("francés", &[("francés", Noun), ("francés", Adj)]),
    ("ve", &[("ir", Verb), ("ver", Verb)]),
    ("menos", &[("menos", Adv), ("menos", Adp), ("menos", Det)]),
    ("bastante", &[("bastante", Adv), ("bastante", Det)]),
    ("fuerte", &[("fuerte", Adj), ("fuerte", Adv)]),
    ("ven", &[("venir", Verb), ("ver", Verb)]),
    ("claro", &[("claro", Intj), ("claro", Adj), ("claro", Adv)]),
    ("rápido", &[("rápido", Adv), ("rápido", Adj)]),
    ("joven", &[("joven", Adj), ("joven", Noun)]),
    ("vale", &[("valer", Verb), ("vale", Intj)]),
    ("ayuda", &[("ayuda", Noun), ("ayudar", Verb)]),
    (
        "seguro",
        &[("seguro", Adj), ("seguro", Adv), ("seguro", Noun)],
    ),
    ("odio", &[("odiar", Verb), ("odio", Noun)]),
    ("tarde", &[("tarde", Adv), ("tarde", Noun)]),
    (
        "extraño",
        &[("extraño", Adj), ("extrañar", Verb), ("extraño", Noun)],
    ),
    ("justo", &[("justo", Adv), ("justo", Adj)]),
    ("fui", &[("ser", Aux), ("ir", Verb)]),
    ("recuerdo", &[("recordar", Verb), ("recuerdo", Noun)]),
    ("bajo", &[("bajo", Adp), ("bajo", Adj)]),
    ("poco", &[("poco", Adv), ("poco", Det), ("poco", Pron)]),
    ("vaya", &[("vaya", Intj), ("ir", Verb)]),
    ("frío", &[("frío", Noun), ("frío", Adj)]),
    ("vivo", &[("vivo", Adj), ("vivir", Verb)]),
    ("blanco", &[("blanco", Adj), ("blanco", Noun)]),
    ("peor", &[("peor", Adj), ("peor", Adv)]),
    ("muerto", &[("muerto", Adj), ("morir", Verb)]),
    ("duro", &[("duro", Adj), ("duro", Adv)]),
    ("negro", &[("negro", Adj), ("negro", Noun)]),
    ("adelante", &[("adelante", Adv), ("adelante", Intj)]),
    ("chico", &[("chico", Noun), ("chico", Adj)]),
    ("pregunta", &[("pregunta", Noun), ("preguntar", Verb)]),
    ("japonés", &[("japonés", Noun), ("japonés", Adj)]),
    ("rojo", &[("rojo", Adj), ("rojo", Noun)]),
    ("alto", &[("alto", Adj), ("alto", Adv)]),
    ("amo", &[("amar", Verb), ("amo", Noun)]),
    ("atención", &[("atención", Noun), ("atención", Intj)]),
    ("cara", &[("cara", Noun), ("caro", Adj)]),
    ("alemán", &[("alemán", Noun), ("alemán", Adj)]),
];

const GERMAN: &[PolysemousWord] = &[
    (
        "was",
        &[("was", Pron), ("etwas", Pron), ("was", Intj), ("was", Det)],
    ),
    ("aber", &[("aber", Cconj), ("aber", Adv), ("aber", Part)]),
    ("so", &[("so", Adv), ("so", Det), ("so", Intj)]),
    ("noch", &[("noch", Adv), ("noch", Cconj)]),
    (
        "bitte",
        &[("bitte", Intj), ("bitte", Adv), ("bitten", Verb)],
    ),
    ("ja", &[("ja", Intj), ("ja", Part), ("ja", Adv)]),
    (
        "doch",
        &[
            ("doch", Part),
            ("doch", Adv),
            ("doch", Cconj),
            ("doch", Intj),
        ],
    ),
    ("etwas", &[("etwas", Pron), ("etwas", Det), ("etwas", Adv)]),
    ("gut", &[("gut", Adv), ("gut", Adj), ("gut", Intj)]),
    (
        "mehr",
        &[("mehr", Adv), ("mehr", Det), ("viel", Adv), ("mehr", Pron)],
    ),
    ("viel", &[("viel", Adv), ("viel", Det), ("viel", Pron)]),
    ("denn", &[("denn", Part), ("denn", Adv), ("denn", Cconj)]),
    ("ab", &[("ab", Part), ("ab", Intj)]),
    ("einfach", &[("einfach", Adv), ("einfach", Adj)]),
    ("weg", &[("weg", Adv), ("weg", Part), ("weg", Intj)]),
    (
        "los",
        &[("los", Intj), ("los", Part), ("los", Adj), ("los", Adv)],
    ),
    ("allein", &[("allein", Adv), ("allein", Adj)]),
    ("gehört", &[("hören", Verb), ("gehören", Verb)]),
    ("besser", &[("gut", Adv), ("gut", Adj)]),
    ("gleich", &[("gleich", Adv), ("gleich", Adj)]),
    ("schnell", &[("schnell", Adv), ("schnell", Adj)]),
    (
        "sieht",
        &[("sehen", Verb), ("aussehen", Verb), ("fernsehen", Verb)],
    ),
    ("verloren", &[("verlieren", Verb), ("verloren", Adj)]),
    (
        "sah",
        &[("sehen", Verb), ("aussehen", Verb), ("ansehen", Verb)],
    ),
    ("nun", &[("nun", Adv), ("nun", Intj)]),
    ("weiß", &[("wissen", Verb), ("weiß", Adj)]),
    ("raus", &[("raus", Part), ("raus", Adv), ("raus", Intj)]),
    ("sicher", &[("sicher", Adv), ("sicher", Adj)]),
    (
        "richtig",
        &[("richtig", Adv), ("richtig", Adj), ("richtig", Intj)],
    ),
    ("schön", &[("schön", Adj), ("schön", Adv)]),
    ("mag", &[("mögen", Verb), ("mögen", Aux)]),
    ("genau", &[("genau", Adv), ("genau", Intj)]),
    ("gefallen", &[("gefallen", Verb), ("fallen", Verb)]),
    ("okay", &[("okay", Intj), ("okay", Adj)]),
    ("recht", &[("Recht", Noun), ("recht", Adv), ("recht", Adj)]),
    ("klar", &[("klar", Adj), ("klar", Intj), ("klar", Adv)]),
    ("langsam", &[("langsam", Adv), ("langsam", Adj)]),
    ("genug", &[("genug", Adv), ("genug", Det), ("genug", Pron)]),
    ("schlecht", &[("schlecht", Adj), ("schlecht", Adv)]),
    ("echt", &[("echt", Adv), ("echt", Adj)]),
    ("ruhig", &[("ruhig", Adj), ("ruhig", Adv)]),
    ("leicht", &[("leicht", Adv), ("leicht", Adj)]),
    ("stand", &[("stehen", Verb), ("aufstehen", Verb)]),
    ("kurz", &[("kurz", Adv), ("kurz", Adj)]),
    ("verdammt", &[("verdammt", Intj), ("verdammt", Adv)]),
    ("hört", &[("hören", Verb), ("aufhören", Verb)]),
    ("lange", &[("lang", Adv), ("lang", Adj)]),
    ("eins", &[("eins", Num), ("eins", Pron)]),
    ("schwer", &[("schwer", Adj), ("schwer", Adv)]),
    (
        "bestimmt",
        &[("bestimmt", Adv), ("bestimmen", Verb), ("bestimmt", Adj)],
    ),
    ("geboren", &[("gebären", Verb), ("geboren", Adj)]),
    ("anders", &[("anders", Adv), ("anders", Adj)]),
    ("falsch", &[("falsch", Adv), ("falsch", Adj)]),
    ("nahm", &[("nehmen", Verb), ("annehmen", Verb)]),
    ("verliebt", &[("verliebt", Adj), ("verlieben", Verb)]),
    ("stark", &[("stark", Adj), ("stark", Adv)]),
    ("verletzt", &[("verletzen", Verb), ("verletzen", Adj)]),
    ("voll", &[("voll", Adj), ("voll", Adv)]),
    ("schau", &[("schauen", Verb), ("anschauen", Verb)]),
    ("weit", &[("weit", Adv), ("weit", Adj)]),
    ("sieh", &[("sehen", Verb), ("ansehen", Verb)]),
    ("kommt", &[("kommen", Verb), ("ankommen", Verb)]),
    ("geht", &[("gehen", Verb), ("angehen", Verb)]),
    ("kam", &[("kommen", Verb), ("ankommen", Verb)]),
];

const ITALIAN: &[PolysemousWord] = &[
    ("come", &[("come", Adv), ("come", Adp), ("come", Sconj)]),
    ("si", &[("si", Pron), ("si", Intj)]),
    ("ora", &[("ora", Adv), ("ora", Noun)]),
    ("cosa", &[("cosa", Pron), ("cosa", Noun)]),
    ("sei", &[("essere", Aux), ("sei", Num)]),
    ("solo", &[("solo", Adv), ("solo", Adj)]),
    ("no", &[("no", Intj), ("no", Adv)]),
    ("bene", &[("bene", Adv), ("bene", Intj), ("bene", Noun)]),
    ("tutto", &[("tutto", Pron), ("tutto", Det), ("tutto", Adv)]),
    ("sì", &[("sì", Intj), ("sì", Adv)]),
    ("su", &[("su", Adp), ("su", Intj), ("su", Adv)]),
    ("vero", &[("vero", Intj), ("vero", Adj)]),
    ("prima", &[("prima", Adv), ("primo", Adj), ("prima", Adp)]),
    ("fa", &[("fare", Verb), ("fa", Adv)]),
    ("troppo", &[("troppo", Adv), ("troppo", Det)]),
    ("fuori", &[("fuori", Adv), ("fuori", Adp)]),
    ("dai", &[("dare", Verb), ("dai", Intj), ("da il", Adp)]),
    ("già", &[("già", Adv), ("già", Intj)]),
    ("via", &[("via", Adv), ("via", Intj), ("via", Noun)]),
    ("cazzo", &[("cazzo", Intj), ("cazzo", Noun)]),
    ("male", &[("male", Adv), ("male", Noun)]),
    ("grazie", &[("grazie", Intj), ("grazia", Noun)]),
    ("dopo", &[("dopo", Adp), ("dopo", Adv)]),
    ("proprio", &[("proprio", Adv), ("proprio", Det)]),
    (
        "giusto",
        &[("giusto", Adj), ("giusto", Intj), ("giusto", Adv)],
    ),
    ("lavoro", &[("lavoro", Noun), ("lavorare", Verb)]),
    ("forte", &[("forte", Adj), ("forte", Adv)]),
    ("porta", &[("portare", Verb), ("porta", Noun)]),
    ("dentro", &[("dentro", Adv), ("dentro", Adp)]),
    ("tanto", &[("tanto", Adv), ("tanto", Det)]),
    ("certo", &[("certo", Intj), ("certo", Adv), ("certo", Adj)]),
    ("avanti", &[("avanti", Intj), ("avanti", Adv)]),
    ("faccia", &[("fare", Verb), ("faccia", Noun)]),
    ("forza", &[("forza", Intj), ("forza", Noun)]),
    ("francese", &[("francese", Noun), ("francese", Adj)]),
    ("fatto", &[("fare", Verb), ("fatto", Noun)]),
    ("altro", &[("altro", Pron), ("altro", Det), ("altro", Adj)]),
    ("abbastanza", &[("abbastanza", Adv), ("abbastanza", Det)]),
    ("sotto", &[("sotto", Adp), ("sotto", Adv)]),
    (
        "stato",
        &[("essere", Aux), ("essere", Verb), ("stato", Noun)],
    ),
    ("giovane", &[("giovane", Adj), ("giovane", Noun)]),
    ("parte", &[("parte", Noun), ("partire", Verb)]),
    ("scusa", &[("scusa", Intj), ("scusa", Noun)]),
    ("vecchio", &[("vecchio", Adj), ("vecchio", Noun)]),
    ("piacere", &[("piacere", Noun), ("piacere", Verb)]),
    ("caldo", &[("caldo", Adj), ("caldo", Noun)]),
    ("bravo", &[("bravo", Adj), ("bravo", Intj)]),
    ("vivo", &[("vivo", Adj), ("vivere", Verb)]),
    ("freddo", &[("freddo", Noun), ("freddo", Adj)]),
    ("letto", &[("letto", Noun), ("leggere", Verb)]),
    ("inglese", &[("inglese", Noun), ("inglese", Adj)]),
    ("ricordi", &[("ricordare", Verb), ("ricordo", Noun)]),
    ("veloce", &[("veloce", Adj), ("veloce", Adv)]),
    ("aiuto", &[("aiuto", Noun), ("aiutare", Verb)]),
    ("meno", &[("meno", Adv), ("meno", Det)]),
    ("sposato", &[("sposato", Adj), ("sposare", Verb)]),
    ("ricordo", &[("ricordare", Verb), ("ricordo", Noun)]),
    ("piano", &[("piano", Noun), ("piano", Adv)]),
    ("poco", &[("poco", Adv), ("poco", Det)]),
    ("blu", &[("blu", Adj), ("blu", Noun)]),
    ("sbagliato", &[("sbagliato", Adj), ("sbagliare", Verb)]),
    ("bianco", &[("bianco", Adj), ("bianco", Noun)]),
    ("prego", &[("prego", Intj), ("pregare", Verb)]),
    ("signore", &[("signore", Noun), ("signora", Noun)]),
];

const PORTUGUESE: &[PolysemousWord] = &[
    ("não", &[("não", Adv), ("não", Intj)]),
    ("é", &[("ser", Aux), ("é", Intj)]),
    (
        "como",
        &[
            ("como", Adv),
            ("como", Adp),
            ("como", Sconj),
            ("comer", Verb),
        ],
    ),
    ("mais", &[("mais", Adv), ("mais", Det), ("mais", Cconj)]),
    (
        "foi",
        &[("ser", Aux), ("ir", Verb), ("ir", Aux), ("ser", Verb)],
    ),
    ("só", &[("só", Adv), ("só", Adj)]),
    ("bem", &[("bem", Adv), ("bem", Intj)]),
    ("vamos", &[("ir", Aux), ("ir", Verb), ("vamos", Intj)]),
    ("sim", &[("sim", Intj), ("sim", Adv)]),
    ("nada", &[("nada", Pron), ("nada", Adv), ("nadar", Verb)]),
    ("mesmo", &[("mesmo", Adv), ("mesmo", Adj), ("mesmo", Det)]),
    ("certo", &[("certo", Adj), ("certo", Intj)]),
    ("nem", &[("nem", Adv), ("nem", Cconj)]),
    ("até", &[("até", Adp), ("até", Adv)]),
    ("melhor", &[("bom", Adj), ("bem", Adv)]),
    ("cara", &[("cara", Noun), ("cara", Intj), ("caro", Adj)]),
    (
        "quanto",
        &[
            ("quanto", Adv),
            ("quanto", Sconj),
            ("quanto", Cconj),
            ("quanto", Pron),
        ],
    ),
    ("bom", &[("bom", Adj), ("bom", Intj)]),
    ("fala", &[("falar", Verb), ("fala", Noun)]),
    ("nossa", &[("nosso", Det), ("nossa", Intj), ("nosso", Pron)]),
    ("francês", &[("francês", Noun), ("francês", Adj)]),
    ("tanto", &[("tanto", Adv), ("tanto", Det)]),
    ("mal", &[("mal", Adv), ("mal", Noun)]),
    ("rápido", &[("rápido", Adv), ("rápido", Adj)]),
    ("primeiro", &[("primeiro", Adv), ("primeiro", Adj)]),
    ("inglês", &[("inglês", Noun), ("inglês", Adj)]),
    ("ajuda", &[("ajuda", Noun), ("ajudar", Verb)]),
    ("todo", &[("todo", Det), ("todo", Adv)]),
    ("trabalho", &[("trabalho", Noun), ("trabalhar", Verb)]),
    ("porra", &[("porra", Intj), ("porra", Noun)]),
    ("velho", &[("velho", Adj), ("velho", Noun)]),
    ("né", &[("né", Intj), ("né", Part)]),
    ("entre", &[("entre", Adp), ("entrar", Verb)]),
    ("claro", &[("claro", Intj), ("claro", Adj), ("claro", Adv)]),
    ("pessoal", &[("pessoal", Noun), ("pessoal", Adj)]),
    ("bastante", &[("bastante", Adv), ("bastante", Det)]),
    ("desculpe", &[("desculpar", Verb), ("desculpe", Intj)]),
    ("menos", &[("menos", Adv), ("menos", Adp), ("menos", Det)]),
    ("merda", &[("merda", Noun), ("merda", Intj)]),
    ("idiota", &[("idiota", Noun), ("idiota", Adj)]),
    ("conta", &[("conta", Noun), ("contar", Verb)]),
    ("fosse", &[("ser", Aux), ("ir", Aux), ("ir", Verb)]),
    ("forte", &[("forte", Adj), ("forte", Adv)]),
    ("tarde", &[("tarde", Adv), ("tarde", Noun)]),
    ("errado", &[("errado", Adj), ("errado", Adv)]),
    ("espera", &[("esperar", Verb), ("espera", Noun)]),
    ("jovem", &[("jovem", Adj), ("jovem", Noun)]),
    ("andar", &[("andar", Verb), ("andar", Noun)]),
    ("escolha", &[("escolha", Noun), ("escolher", Verb)]),
    ("leve", &[("levar", Verb), ("leve", Adj)]),
    ("sei", &[("saber", Verb), ("saber", Aux)]),
    ("pergunta", &[("pergunta", Noun), ("perguntar", Verb)]),
    ("jogo", &[("jogo", Noun), ("jogar", Verb)]),
    ("azul", &[("azul", Adj), ("azul", Noun)]),
    ("presente", &[("presente", Noun), ("presente", Adj)]),
    ("vivo", &[("vivo", Adj), ("viver", Verb)]),
    ("alto", &[("alto", Adj), ("alto", Adv)]),
    ("sério", &[("sério", Adj), ("sério", Intj)]),
    ("atenção", &[("atenção", Noun), ("atenção", Intj)]),
    ("alemão", &[("alemão", Noun), ("alemão", Adj)]),
    ("seguro", &[("seguro", Adj), ("seguro", Noun)]),
    ("saia", &[("sair", Verb), ("saia", Noun)]),
    ("branco", &[("branco", Adj), ("branco", Noun)]),
    ("vestido", &[("vestido", Noun), ("vestido", Adj)]),
    ("vermelho", &[("vermelho", Adj), ("vermelho", Noun)]),
    ("parte", &[("parte", Noun), ("partir", Verb)]),
];

const CHINESE: &[PolysemousWord] = &[
    ("会", &[("会", Aux), ("会", Verb), ("会", Noun)]), // can/will / meet / meeting
    ("要", &[("要", Aux), ("要", Verb), ("要", Adj)]),  // want/will / want / important
    ("就", &[("就", Adv), ("就", Cconj)]),              // then/just / then
    ("还", &[("还", Adv), ("还", Verb)]),               // still/also / return
    ("过", &[("过", Part), ("过", Verb)]),              // experiential aspect / to pass
    ("得", &[("得", Part), ("得", Verb)]),              // complement marker / to get
    ("给", &[("给", Verb), ("给", Adp)]),               // to give / to/for
    ("到", &[("到", Verb), ("到", Adp)]),               // to arrive / to
    ("把", &[("把", Adp), ("把", Noun)]),               // BA-construction / handle
    ("对", &[("对", Adj), ("对", Adp), ("对", Verb)]),  // correct / toward / to face
    ("让", &[("让", Verb), ("让", Adp)]),               // to let / by (passive)
    ("想", &[("想", Verb), ("想", Aux)]),               // to think / to want
    ("点", &[("点", Noun), ("点", Verb), ("点", Num)]), // point / to order / a little
    ("行", &[("行", Adj), ("行", Verb), ("行", Noun)]), // OK / to walk / row
    ("回", &[("回", Verb), ("回", Noun)]),              // to return / time(s)
    ("多", &[("多", Adj), ("多", Adv)]),                // many / how (much)
    ("快", &[("快", Adj), ("快", Adv)]),                // fast / soon
    ("长", &[("长", Adj), ("长", Verb)]),               // long / to grow
    ("好", &[("好", Adj), ("好", Adv), ("好", Intj)]),  // good / very / OK!
    ("地", &[("地", Part), ("地", Noun)]),              // adverbial marker / earth
    ("下", &[("下", Verb), ("下", Noun), ("下", Adp)]), // to go down / below / under
    ("上", &[("上", Verb), ("上", Noun), ("上", Adp)]), // to go up / above / on
    ("跟", &[("跟", Adp), ("跟", Verb), ("跟", Cconj)]), // with / to follow / and
];

const JAPANESE: &[PolysemousWord] = &[
    ("もう", &[("もう", Adv), ("もう", Intj)]), // already / geez!
    ("まだ", &[("まだ", Adv), ("まだ", Intj)]), // still/not yet
    ("ちょっと", &[("ちょっと", Adv), ("ちょっと", Intj)]), // a little / hey!
    ("本当", &[("本当", Noun), ("本当", Adv)]), // truth / really
    ("大丈夫", &[("大丈夫", Adj), ("大丈夫", Intj)]), // OK (adj) / OK! (intj)
    ("結構", &[("結構", Adv), ("結構", Adj)]),  // quite / fine/enough
    ("方", &[("方", Noun), ("方", Noun)]),      // direction / person (polite)
    ("前", &[("前", Noun), ("前", Adp)]),       // front / before
    ("後", &[("後", Noun), ("後", Adp)]),       // back / after
    ("間", &[("間", Noun), ("間", Adp)]),       // between / during
    ("一番", &[("一番", Adv), ("一番", Noun)]), // most / number one
    ("自分", &[("自分", Noun), ("自分", Pron)]), // oneself / I (informal)
    ("いい", &[("いい", Adj), ("いい", Intj)]), // good / OK!/fine!
    ("よく", &[("よく", Adv), ("よい", Adj)]),  // often/well / good (adverbial)
    ("よう", &[("よう", Noun), ("よう", Aux)]), // appearance / seeming
    ("気", &[("気", Noun), ("気", Noun)]),      // spirit/feeling / care/attention
    ("物", &[("物", Noun), ("物", Noun)]),      // thing / person
    ("事", &[("事", Noun), ("事", Noun)]),      // thing/matter / experience
    ("所", &[("所", Noun), ("所", Noun)]),      // place / point/aspect
    ("中", &[("中", Noun), ("中", Adp)]),       // inside / during/in the middle of
    ("上", &[("上", Noun), ("上", Adp)]),       // top/above / in terms of
    ("下", &[("下", Noun), ("下", Adp)]),       // below / under
    ("先", &[("先", Noun), ("先", Adv)]),       // destination/tip / first/ahead
    ("外", &[("外", Noun), ("外", Adp)]),       // outside / besides
    ("通り", &[("通り", Noun), ("通り", Adp)]), // street / as/like
    ("代わり", &[("代わり", Noun), ("代わり", Adp)]), // substitute / instead of
    ("やっぱり", &[("やっぱり", Adv), ("やっぱり", Intj)]), // as expected / I knew it!
    ("確か", &[("確か", Adj), ("確か", Adv)]),  // certain / if I recall
    ("別", &[("別", Adj), ("別", Noun)]),       // separate / farewell
    ("無理", &[("無理", Adj), ("無理", Noun)]), // impossible / unreasonableness
    ("駄目", &[("駄目", Adj), ("駄目", Intj)]), // no good / no!/stop!
    ("違う", &[("違う", Verb), ("違う", Intj)]), // to differ / no! (disagreement)
    ("全然", &[("全然", Adv), ("全然", Adv)]),  // not at all / totally (colloquial)
    ("全部", &[("全部", Noun), ("全部", Adv)]), // everything / entirely
    ("一度", &[("一度", Adv), ("一度", Noun)]), // once / one time
    ("急", &[("急", Adj), ("急", Noun)]),       // sudden / urgency
    ("丁寧", &[("丁寧", Adj), ("丁寧", Noun)]), // polite / politeness
    ("邪魔", &[("邪魔", Noun), ("邪魔", Adj)]), // hindrance / in the way
    ("適当", &[("適当", Adj), ("適当", Noun)]), // appropriate / carelessness
    ("勝手", &[("勝手", Adj), ("勝手", Noun)]), // selfish / one's own way
    ("変", &[("変", Adj), ("変", Noun)]),       // strange / change
    ("簡単", &[("簡単", Adj), ("簡単", Noun)]), // easy / simplicity
    ("最初", &[("最初", Noun), ("最初", Adv)]), // beginning / at first
    ("最後", &[("最後", Noun), ("最後", Adv)]), // end / lastly
    ("今度", &[("今度", Noun), ("今度", Adv)]), // next time / this time
    ("絶対", &[("絶対", Adv), ("絶対", Noun)]), // absolutely / absolute
    ("特別", &[("特別", Adj), ("特別", Adv)]),  // special / especially
    ("普通", &[("普通", Adj), ("普通", Adv)]),  // normal / normally
    ("勉強", &[("勉強", Noun), ("勉強", Verb)]), // study / to study
    ("練習", &[("練習", Noun), ("練習", Verb)]), // practice / to practice
    ("心配", &[("心配", Noun), ("心配", Adj)]), // worry / worried
    ("迷惑", &[("迷惑", Noun), ("迷惑", Adj)]), // trouble / troublesome
    ("我慢", &[("我慢", Noun), ("我慢", Verb)]), // patience / to endure
    ("上手", &[("上手", Adj), ("上手", Noun)]), // skillful / skill
    ("下手", &[("下手", Adj), ("下手", Noun)]), // unskillful / poor skill
    ("つもり", &[("つもり", Noun), ("つもり", Noun)]), // intention / belief (mistaken)
    ("さすが", &[("さすが", Adv), ("さすが", Intj)]), // as expected / wow, impressive!
    ("なかなか", &[("なかなか", Adv), ("なかなか", Adv)]), // quite/fairly / not easily (with neg)
    ("ほど", &[("ほど", Adp), ("ほど", Noun)]), // about/to the extent / degree
    ("はず", &[("はず", Noun), ("はず", Noun)]), // should be/expected / rim/edge
    ("わけ", &[("わけ", Noun), ("わけ", Noun)]), // reason / meaning/situation
];

const HINDI: &[PolysemousWord] = &[
    ("सब", &[("सब", Det), ("सब", Pron), ("सब", Noun)]), // all (det/pron) / everyone (noun)
    ("और", &[("और", Cconj), ("और", Adv)]),              // and / more
    ("बस", &[("बस", Adv), ("बस", Noun), ("बस", Intj)]), // just / bus / enough!
    ("पर", &[("पर", Adp), ("पर", Cconj)]),              // on / but
    ("तो", &[("तो", Part), ("तो", Cconj)]),             // then/emphasis / then/so
    ("भी", &[("भी", Part), ("भी", Adv)]),               // also / even
    ("ही", &[("ही", Part), ("ही", Adv)]),               // only/emphasis / only
    ("अच्छा", &[("अच्छा", Adj), ("अच्छा", Intj)]),         // good / OK!/I see
    ("ठीक", &[("ठीक", Adj), ("ठीक", Intj), ("ठीक", Adv)]), // fine / OK! / exactly
    ("बहुत", &[("बहुत", Adv), ("बहुत", Det)]),             // very / much/many
    ("कुछ", &[("कुछ", Pron), ("कुछ", Det), ("कुछ", Adv)]),  // something / some / somewhat
    ("यह", &[("यह", Pron), ("यह", Det)]),               // this (pron) / this (det)
    ("वह", &[("वह", Pron), ("वह", Det)]),               // that (pron) / that (det)
    ("क्या", &[("क्या", Pron), ("क्या", Part), ("क्या", Adv)]), // what / question marker / how
    ("बड़ा", &[("बड़ा", Adj), ("बड़ा", Adv)]),             // big / very
    ("ज़रा", &[("ज़रा", Adv), ("ज़रा", Det)]),             // a little / a bit of
    ("सीधा", &[("सीधा", Adj), ("सीधा", Adv)]),          // straight / directly
    ("खुद", &[("खुद", Pron), ("खुद", Adv)]),               // oneself / personally
    ("बिल्कुल", &[("बिल्कुल", Adv), ("बिल्कुल", Intj)]),      // absolutely / exactly!
    ("चलो", &[("चलना", Verb), ("चलो", Intj)]),          // let's go / come on!
    ("देखो", &[("देखना", Verb), ("देखो", Intj)]),          // look (verb) / look!/listen!
    ("सच", &[("सच", Noun), ("सच", Adj), ("सच", Adv)]),  // truth / true / truly
    ("ज़रूरी", &[("ज़रूरी", Adj), ("ज़रूरी", Adv)]),          // necessary / necessarily
    ("पूरा", &[("पूरा", Adj), ("पूरा", Adv)]),             // complete / completely
    ("साफ़", &[("साफ़", Adj), ("साफ़", Adv)]),             // clean / clearly
    ("अलग", &[("अलग", Adj), ("अलग", Adv)]),             // separate / separately
    ("बराबर", &[("बराबर", Adj), ("बराबर", Adv)]),       // equal / equally
    ("ऊपर", &[("ऊपर", Adv), ("ऊपर", Adp)]),             // above / on top of
    ("नीचे", &[("नीचे", Adv), ("नीचे", Adp)]),             // below / under
    ("आगे", &[("आगे", Adv), ("आगे", Adp)]),                // ahead / in front of
    ("पीछे", &[("पीछे", Adv), ("पीछे", Adp)]),             // behind / after
    ("बाहर", &[("बाहर", Adv), ("बाहर", Adp)]),          // outside / out of
    ("अंदर", &[("अंदर", Adv), ("अंदर", Adp)]),             // inside / within
    ("बीच", &[("बीच", Noun), ("बीच", Adp)]),            // middle / among/between
    ("बाद", &[("बाद", Noun), ("बाद", Adp)]),            // later / after
    ("पहले", &[("पहले", Adv), ("पहले", Adp)]),             // first/before (adv) / before (adp)
    ("दूर", &[("दूर", Adv), ("दूर", Adj)]),                // far / distant
    ("पास", &[("पास", Adv), ("पास", Adp), ("पास", Noun)]), // nearby / near / pass/possession
    ("काम", &[("काम", Noun), ("काम", Noun)]),           // work / use/function
    ("हाल", &[("हाल", Noun), ("हाल", Adj)]),            // condition / recent
    ("मतलब", &[("मतलब", Noun), ("मतलब", Noun)]),        // meaning / selfishness
    ("खाना", &[("खाना", Verb), ("खाना", Noun)]),        // to eat / food
    ("पानी", &[("पानी", Noun), ("पानी", Noun)]),        // water / tears (poetic)
    ("सवाल", &[("सवाल", Noun), ("सवाल", Noun)]),        // question / matter/issue
    ("जवाब", &[("जवाब", Noun), ("जवाब", Noun)]),        // answer / response/retort
    ("ताज़ा", &[("ताज़ा", Adj), ("ताज़ा", Adj)]),          // fresh / recent/latest
    ("गरम", &[("गरम", Adj), ("गरम", Adj)]),             // hot / heated (angry)
    ("ठंडा", &[("ठंडा", Adj), ("ठंडा", Adj)]),             // cold / cool/calm
    ("हल्का", &[("हल्का", Adj), ("हल्का", Adj)]),          // light (weight) / light (color/mild)
    ("भारी", &[("भारी", Adj), ("भारी", Adj)]),          // heavy / serious/overwhelming
    ("गहरा", &[("गहरा", Adj), ("गहरा", Adj)]),          // deep / intense/profound
    ("मज़बूत", &[("मज़बूत", Adj), ("मज़बूत", Adj)]),          // strong / firm/resolute
    ("कड़ा", &[("कड़ा", Adj), ("कड़ा", Noun)]),            // hard/strict / bangle
    ("मीठा", &[("मीठा", Adj), ("मीठा", Noun)]),         // sweet / sweets/dessert
    ("फिर", &[("फिर", Adv), ("फिर", Cconj)]),           // again / then
    ("अभी", &[("अभी", Adv), ("अभी", Adv)]),             // right now / just now / soon
    ("तक", &[("तक", Adp), ("तक", Part)]),               // until / even
    ("सिर्फ़", &[("सिर्फ़", Adv), ("सिर्फ़", Det)]),          // only (adv) / only (det)
    ("कभी", &[("कभी", Adv), ("कभी", Adv)]),             // ever / sometimes
    ("एक", &[("एक", Det), ("एक", Num)]),                // a/an (article) / one (number)
];
