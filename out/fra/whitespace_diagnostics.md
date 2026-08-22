# Whitespace Prediction Diagnostics

Total predictions: 2971361
Total errors: 1064
Accuracy: 99.96%

## Error Patterns (sorted by frequency)

### "'" + "?" (18 occurrences)
- Predicted: None
- Actual: NarrowNbsp
- Examples:
  - Alors, tu disais 'meilleures intentions' ?
  - C'est donc le célèbre 'Livre des Frères' ?
  - C'est donc le fameux 'Livre des Frères' ?

### "c" + "‘" (11 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Abdel, c‘est pas une caillera.
  - Il paraît que c‘est toi qui as mon argent ?
  - Mais le problème, c‘est que, faut se soulager, faut chier.

### "C" + "‘" (10 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C‘est bon, Saïd, ramène-toi.
  - C‘est joli.
  - C‘est les pires.

### "," + "non" (8 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - C‘est toi qui as le calibre, non ?
  - C’est intéressant, non ?
  - Elle est super, non ?

### "qu" + "‘" (8 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et qu‘on ne les y reprenne plus.
  - Il paraît qu‘un keuf a perdu son calibre dans la cité, cette nuit.
  - Je connais pas le keuf qu‘a perdu le flingue.

### "non" + "?" (7 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - C‘est toi qui as le calibre, non ?
  - C’est intéressant, non ?
  - Elle est super, non ?

### "T" + "‘" (6 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - T‘as des leçons de morale à donner ?
  - T‘as même pas le droit de la regarder.
  - T‘as un numéro de téléphone ?

### "l" + "‘" (6 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C‘est l‘atterrissage.
  - Il a pas l‘air méchant, dans son cuir en peau de fesse de chèvre.
  - On peut l‘enlever avec une pelle ou laisser la pluie et le vent la balayer.

### "nous" + ";" (5 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Juste entre nous; es-tu amoureuse de ma sœur ?
  - Juste entre nous; es-tu amoureux de ma sœur ?
  - Juste entre nous; êtes-vous amoureuse de ma sœur ?

### "," + "'" (4 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment ça, 'encore' ?
  - Comment ça, 'nous' ?
  - Je passe après 'tasse', 'piscine' et 'girafe'.

### "," + "c’" (4 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mener la mission, c’est notre devoir.
  - Tanjiro, si tu peux encore bouger, c’est le moment !
  - Tous pour un, un pour tous, c’est notre devise.

### "-" + "Salut" (4 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Avec de tels pouvoirs, ça ne rigole pas - Salut, Klaus.
  - De toute façon, merci, au revoir, - Salut Aman.
  - Salut - Salut.

### "-" + "le" (4 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il y a trop de machisme en tauromachie, reconnaissez- le.
  - Je perds connaissance, et au réveil - le bras dans le plâtre.
  - Vous avez perdu connaissance, et au réveil - le bras dans le plâtre.

### "J" + "‘" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J‘ai fêté mon anniversaire dans un restaurant.
  - J‘aime beaucoup les chatons.
  - J‘aime celui-ci.

### "d" + "‘" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ces émeutes font suite à la bavure d‘un inspecteur des Muguets.
  - Espèce d‘enculé.
  - Ils ont le droit d‘avoir des calibres aussi gros ?

### "t" + "‘" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je t‘ai invité au Grec, la semaine dernière.
  - Je t‘ai parlé à toi ?
  - Qui t‘es, toi ?

### "—" + "moi" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ah oui, nique—moi ?
  - Excusez—moi !
  - Hé, Hubert, attends—moi là, oh !

### "'" + "en" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - C'est 'île' en espagnol mais à l'envers.
  - Comment dit-on 'ami' en elfe ?
  - Comment dit-on 'planque' en français ?

### "'" + "à" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Place un 'X' à l'endroit opportun.
  - Si on donnait une touche 'irlandaise' à ces cafés ?
  - T'aurais pas un 'R' à me prêter ?

### "," + "hein" (3 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Et toujours la même expression, hein ?
  - Magnifique, hein ?
  - Vous ne comprenez pas, hein ?

### "," + "monsieur" (3 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Comment fait-on pour séparer son âme en deux, monsieur ?
  - Quel stylo, monsieur ?
  - Savez vous ce que vous prendrez ce soir, monsieur ?

### "," + "«" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment ça, « Quoi ?
  - La première exclamation de John était, « Ha-ha !
  - Tu connais ce mot, « travailler » ?

### "-" + "ce" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Est- ce que je dois entrer par derrière ?
  - Est- ce que tu veux regarder un film ?
  - Je me suis évanoui et quand je reviens à moi - ce plâtre.

### "--" + "!" (3 occurrences)
- Predicted: None
- Actual: NarrowNbsp
- Examples:
  - H-Hey-- !
  - Où est-ce que-- !
  - Suzume-- !

### "--" + "Je" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Donc, de toute façon, je-- Je ne sais pas.
  - Il a dit que la philosophie-- Je frime, donc arrête-moi.
  - J'étais marié avec-- Je peux m'asseoir là ?

### "Qu" + "‘" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qu‘est—ce tu cherches ?
  - Qu‘est—ce tu fais avec ton grenaille ?
  - Qu‘est—ce tu peux faire de mieux ?

### "Rire" + "-" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rire - Oui, ma copine.
  - Rire -Gros dossier.
  - Rire -Oui.

### "bien" + "," (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Eh bien , ils sont géants !
  - Très bien , regardez autour de vous.
  - Ça sonne bien, non ?

### "d" + "'" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ce tableau est considéré comme un chef-d'œuvre.
  - Je me casse d'ici.
  - Je veux que tu sortes d'ici.

### "est" + "—" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qu‘est—ce tu cherches ?
  - Qu‘est—ce tu fais avec ton grenaille ?
  - Qu‘est—ce tu peux faire de mieux ?

### "flèche" + ";" (3 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Le temps vole comme une flèche ; les drosophiles aiment une banane.
  - Le temps vole comme une flèche ; un fruit vole comme une banane.
  - Les mouches du temps aiment une flèche ; les drosophiles aiment une banane.

### "hein" + "?" (3 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Et toujours la même expression, hein ?
  - Magnifique, hein ?
  - Vous ne comprenez pas, hein ?

### "monsieur" + "?" (3 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Comment fait-on pour séparer son âme en deux, monsieur ?
  - Quel stylo, monsieur ?
  - Savez vous ce que vous prendrez ce soir, monsieur ?

### "nous" + "'" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'est qui 'nous' ?
  - Comment ça, 'nous' ?
  - Qu'est-ce que tu veux dire par 'nous' ?

### "«" + "ouistiti" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dis « ouistiti ».
  - Dites « ouistiti ».
  - Tout le monde, dites « ouistiti ».

### "ça" + "," (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment tu sais ça, toi ?
  - Ne fais pas ça , mon frère.
  - Tu aimes ça, non ?

### "—" + "ce" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qu‘est—ce tu cherches ?
  - Qu‘est—ce tu fais avec ton grenaille ?
  - Qu‘est—ce tu peux faire de mieux ?

### "'" + "au" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je croyais qu'on avait choisi 'M' au hasard.
  - Je fais un 'chip' au-dessus de l'eau.

### "'" + "et" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J'ai fait 'rappel' et c'était toi.
  - Je passe après 'tasse', 'piscine' et 'girafe'.

### "," + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - De toute façon, merci, au revoir, - Salut Aman.
  - Ouais, - merci monsieur.

### "," + "cours" (2 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Milkha, cours !
  - Paul, cours !

### "," + "ici" (2 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - En avez-vous tous fini, ici ?
  - Les seuls criminels portent les uniformes nazis, ici !

### "," + "papa" (2 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Qu'est-ce qui se passe, papa ?
  - Tu entends, papa ?

### "-" + "!" (2 occurrences)
- Predicted: None
- Actual: NarrowNbsp
- Examples:
  - La poule fait cot cot- !
  - Merd-- !

### "-" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Elles ont été- -Que faites-vous ici bon sang ?
  - Et un-- -Je suis désolé, Mme Tanner.

### "-" + "Oui" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comme le Cachemire - Oui, comme le Cachemire.
  - Rire - Oui, ma copine.

### "-" + "un" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ce qui peut tout animer, tout adoucir, tout colorer - un grand amour.
  - Lui - un salaud, elle - une sainte.

### "-" + "vous" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Avez- vous conclu ?
  - Pas de problème - vous les évacuez.

### "Frères" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'est donc le célèbre 'Livre des Frères' ?
  - C'est donc le fameux 'Livre des Frères' ?

### "Monsieur" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Monsieur , je regrette.
  - Monsieur , tous les escorts se sont enfuis ou ont été abattus.

### "Nation" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Avant la première de 'La Fierté de la Nation'.
  - Le film s'appelle 'La fierté de la Nation'.

### "Non" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Non , non , non !
  - Non , non !

### "Salut" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Salut - Salut.
  - Salut - Salut !

### "allé" + "?" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Où es-tu allé ?
  - Où tout le monde est-il allé ?

### "aussi" + "?" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Celle-là aussi ?
  - Ça aussi ?

### "cheese" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dis 'cheese'.
  - Dites 'cheese'.

### "cours" + "!" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Milkha, cours !
  - Paul, cours !

### "devenir" + "Jedi" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le fils de Skywalker ne doit pas devenirJedi.
  - Pourquoi veux-tu devenirJedi ?

### "i" + "I" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mais qu'iI finisse ce qu'iI a commencé.
  - Mais qu'iI finisse ce qu'iI a commencé.

### "ici" + "?" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - En avez-vous tous fini, ici ?
  - Pourquoi ce rendez-vous ici ?

### "il" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ce n'est pas : 'Où est-il' ?
  - Qui 'il' ?

### "l" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il a fait un sans-faute à l'examen.
  - On a fini les travaux juste avant l'hiver.

### "là" + "." (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hé, chef, regardez qui est là .
  - Seul un miracle pourrait le sortir de là .

### "m" + "‘" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je m‘appelle pas Bocuse.
  - Je m‘appelle pas Malik Oussekine.

### "meilleur" + ";" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Espère le meilleur ; prépare-toi au pire.
  - Espérez le meilleur ; préparez-vous au pire.

### "moi" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je me suis évanoui et quand je reviens à moi - ce plâtre.
  - Quand je reviens à moi - plâtré.

### "non" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hmm non , je te manquerais.
  - Non , non , non !

### "papa" + "?" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Qu'est-ce qui se passe, papa ?
  - Tu entends, papa ?

### "plaît" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dis 's'il te plaît'.
  - Tu as dit 's'il te plaît' ?

### "réveil" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je perds connaissance, et au réveil - le bras dans le plâtre.
  - Vous avez perdu connaissance, et au réveil - le bras dans le plâtre.

### "scientifique" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je suis scientifique , vous vous en souvenez ?
  - Tommy était le scientifique , pas moi.

### "sorti" + "?" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment es-tu sorti ?
  - Quel étudiant est-il sorti ?

### "toi" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et toi , Mark, tu as hâte de devenir papa ?
  - Relève-toi, bordel !

### "toi" + "?" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Est-ce que ça a un sens pour toi ?
  - Sont-elles venues avec toi ?

### "va" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - On y va, on y va , on y va !
  - Ça va, Lance ?

### "vous" + "." (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ca ne devrait pas être trop dur pour vous .
  - Regardez tout autour de vous .

### "vous" + ";" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Je ne priais pas contre vous ; je priais pour vous.
  - Ne dites jamais du mal de vous ; vos amis en diront toujours assez.

### "vous" + "?" (2 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Elle est bien tenue, d'après vous ?
  - Mes hommes se sont-ils bien occupés de vous ?

### "«" + "Avatar" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Connaissez-vous le film « Avatar » ?
  - Tu connais le film « Avatar » ?

### "«" + "au" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Elle a dit « au revoir ».
  - Il a quitté la maison sans dire « au revoir ».

### "«" + "bonjour" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Avez-vous dit « bonjour » ?
  - Tu as dit « bonjour » ?

### "«" + "soit" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je dis « soit », tu dis.
  - Tu dis « soit ».

### "«" + "vieille" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - C’est impoli de l’appeler « vieille peau ».
  - Dis plutôt « vieille dame ».

### "—" + "lui" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vas-y, explose—lui sa mère !
  - Vas-y, tire—lui une balle, si tu veux.

### "—" + "toi" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Retourne—toi sur ton maître.
  - Tire—toi !

### "'" + "Bonne" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Un dur, ce ' Bonne nuit Anderson', alors fais gaffe.

### "'" + "assez" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J'ai trouvé 'Gretchen Ross' assez cool.

### "'" + "d'" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J'entends juste le 'bip' d'une balise.

### "'" + "dans" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Moi et le mot 'devoir' dans la même phrase.

### "'" + "exigent" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Les chevaliers qui disent 'Ni' exigent un sacrifice.

### "'" + "ne" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sauf que 'toi' ne veut plus rien dire.

### "'" + "sur" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J'ai un 'cube du destin' sur moi.

### "'" + "un" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J'ai 'fait une nana' un tas de fois.

### "'" + "vous" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Par 'ces gars' vous voulez dire mes clients ?

### "'i" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - On va mettre les points sur les 'i'.

### "," + "Benni" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ouvre la porte, Benni !

### "," + "Capitaine" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Voulez-vous que je prenne les commandes, Capitaine ?

### "," + "Carmelino" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Non, Carmelino !

### "," + "Dieu" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Que puis-je faire pour Toi, Dieu ?

### "," + "Fabinho" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Oh, Fabinho !

### "," + "Frank" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Hé, Frank !

### "," + "Idéfix" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Au pied, Idéfix !

### "," + "Jasmeet" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Salut, Jasmeet !

### "," + "Jim" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Tu savais, Jim ?

### "," + "Jordan" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Tu veux me baiser, Jordan ?

### "," + "Kimmie" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quatre maudites fois, Kimmie !

### "," + "Lamar" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Qu'allez-vous faire, Lamar ?

### "," + "Lance" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ça va, Lance ?

### "," + "Marla" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Réponds, Marla !

### "," + "Marty" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Yo, Marty !

### "," + "Mikiya" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quoi, Mikiya ?

### "," + "Paulo" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Allô, Paulo ?

### "," + "Pippin" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Et toi, Pippin ?

### "," + "Tignasse" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Et toi, Tignasse ?

### "," + "Tom" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Que fais-tu ce soir, Tom ?

### "," + "Zico" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Tu veux nous faire peur, Zico ?

### "," + "alors" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ne seraient-ils pas malheureux, alors ?

### "," + "bordel" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Relève-toi, bordel !

### "," + "d'" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - C'est quoi, d'ailleurs ?

### "," + "d’" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Que faites-vous chez toi, d’ordinaire ?

### "," + "enfoiré" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Où est ce fils de pute, enfoiré ?

### "," + "général" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Pourquoi ne commencez-vous pas, général ?

### "," + "je" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et moi,je le voyais à la mienne.

### "," + "lui" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Qui, lui ?

### "," + "là" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Tu vois le barbu, là ?

### "," + "maintenant" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - As-tu du temps, maintenant ?

### "," + "maman" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Pas vrai, maman ?

### "," + "mec" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Que foutent ces mines ici, mec ?

### "," + "pourriture" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Regarde-moi dans les yeux, pourriture !

### "," + "quoi" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ils s'amènent toujours la nuit, et nous, quoi ?

### "," + "recule" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Tanjiro, recule !

### "," + "toi" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Comment tu sais ça, toi ?

### "," + "épouvantail" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Regarde-toi toi-même, épouvantail !

### "-" + "Allez" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Folk - Allez, on les a.

### "-" + "Bon" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Alerte sismique - Bon sang !

### "-" + "Bravo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cris de joie - Bravo.

### "-" + "Catherine" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Regarde - Catherine !

### "-" + "Ces" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ces cartes-- Ces cartes sont pourries.

### "-" + "Chien" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je faisais partie de la conspiration - Chien !

### "-" + "Des" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ne manque pas mon jeu, mon chéri - Des rubis ?

### "-" + "Dieu" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oh - Mon - Dieu !

### "-" + "Dis" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il faut neuf ingrédients - Dis nous !

### "-" + "Dénissov" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Natacha, Véra, le voici - Dénissov.

### "-" + "Entrez" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mme Delassalle se repose - Entrez donc !

### "-" + "Es" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Quoi-- Es-tu fâchée ?

### "-" + "Mais" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Désolé, je suis en retard - Mais marche alors.

### "-" + "Merci" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rires - Merci, Éli.

### "-" + "Mon" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oh - Mon - Dieu !

### "-" + "Ne" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Elle a toujours été comme ça - Ne dis pas des choses pareilles.

### "-" + "On" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Touristo russo - On est la morale infuse !

### "-" + "Pardon" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Coups de klaxon - Pardon.

### "-" + "Pas" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Salut, Schutte - Pas de noms !

### "-" + "Quelle" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Soupir - Quelle heure est-il ?

### "-" + "Venez" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Non, vraiment, nous devons y aller - Venez.

### "-" + "alors" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vous avez été amené ici pour travailler - alors travaillez.

### "-" + "avoir" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "-" + "bousculade" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Partout - bousculade, cohue, queue.

### "-" + "chut" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le cochon lui saute dessus - chut.

### "-" + "dans" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oui - dans la norme.

### "-" + "debout" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Debout- debout !

### "-" + "deux" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il est toujours escorté - deux, trois chiens de garde.

### "-" + "directement" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Là, je mettrai le fenouil, le persil, tu vois - directement sur la table.

### "-" + "eh" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Eh oui, mais - eh - les autres ?

### "-" + "elle" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - On lui a raconté des choses, elle a fait des rêves - elle a peur.

### "-" + "gardé" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "-" + "hop" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Avel, allez - hop !

### "-" + "il" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Lâche cinglé - il ne pouvait même pas mordre le foutu.

### "-" + "les" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Eh oui, mais - eh - les autres ?

### "-" + "là" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Votre équipe - là-bas, derrière la tombe.

### "-" + "merci" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ouais, - merci monsieur.

### "-" + "moi" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Excusez- moi.

### "-" + "mon" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "-" + "nous" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment allons- nous nous battre ?

### "-" + "officier" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "-" + "oui" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - De la pelouse vers le lac - oui ?

### "-" + "pas" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Un meurtrier et pire - pas même de compassion pour cette malheureuse.

### "-" + "plat" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Son ventre me rappelle les cartes postales du Japon - plat et joli.

### "-" + "plâtré" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Quand je reviens à moi - plâtré.

### "-" + "pour" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "-" + "rien" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pardonnez toujours à vos ennemis - rien ne les agace plus que cela.

### "-" + "seulement" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La guerre ne décide pas qui a raison - seulement qui est vivant.

### "-" + "sortez" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Traites, pas traites - sortez-les !

### "-" + "telle" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Boire ou ne pas boire - telle est la question.

### "-" + "tiens" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vous creusez là - tiens donc, le noyau atomique est constitué de protons.

### "-" + "toi" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Suicide- toi, suicide-toi, suicide-toi.

### "-" + "tous" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Toi et les autres - tous en tôle.

### "-" + "trouver" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ou--je ne sais pas-- trouver un emploi à plein temps ?

### "-" + "une" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Lui - un salaud, elle - une sainte.

### "-" + "благодать" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Каждая погода - благодать.

### "-" + "–" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Un, deux- – Cul sec.

### "--" + "C'" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cette aventure était bien plus-- C'était très bien.

### "--" + "Comment" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Des métaphores ce sont-- Comment pourrais-je expliquer ?

### "--" + "Dieu" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Chacun pour soi -- Dieu pour tous.

### "--" + "Douglas Kelley" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je suis -- Douglas Kelley.

### "--" + "Non" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je veux dire, je voudrais-- Non.

### "--" + "quels" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Les filets-- quels filets ?

### "--" + "qui" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Leur pouvoir vient de leur leader-- qui est en quelque sorte leur cerveau.

### "--" + "un" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Regardez, Monsieur -- un droïde.

### "<" + "courriel" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Envoyez-nous votre CV détaillé à <courriel>.

### "Accorche" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Accorche -toi !

### "Algérie" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Es-tu rentré en Algérie ?

### "Allez" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Allez , Enfilez vos Exo-packs !

### "Allez" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Allez; rentrons a la maison.

### "Allô" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Allô, Paulo ?

### "Alors" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Alors , arrête-le quand il sortira en ville.

### "Anderson" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Un dur, ce ' Bonne nuit Anderson', alors fais gaffe.

### "Andrea" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Fais attention à ce que tu dis, Andrea; les murs ont des oreilles.

### "Angela" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Angela -Bonjour.

### "Asseyez" + "–" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Asseyez–vous.

### "Aussi" + "vite" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Aussi vite ?

### "Benni" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ouvre la porte, Benni !

### "Bon" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bon , ça y est, le Rouge ?

### "Bonjour" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bonjour -Bonjour, bébé !

### "Cachemire" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comme le Cachemire - Oui, comme le Cachemire.

### "Capitaine" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Voulez-vous que je prenne les commandes, Capitaine ?

### "Carmelino" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Non, Carmelino !

### "Chasseur de juifs" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qu'on vous appelle le 'Chasseur de juifs'.

### "Cherche" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Cherche ; trouve ; découvre !

### "Comment" + "décider" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Comment décider ?

### "Comment" + "va" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Comment va ?

### "Coupez" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Coupe pas tant que je dis pas 'Coupez'.

### "Courir" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Courir , rester , alors quoi ?

### "Des" + "frites" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Des frites ?

### "Des" + "moustiques" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Des moustiques ?

### "Des" + "signatures" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Des signatures ?

### "Des" + "souhaits" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Des souhaits ?

### "Des" + "truands" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Des truands ?

### "Dieu" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Que puis-je faire pour Toi, Dieu ?

### "Du" + "Polynectar" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Du Polynectar ?

### "Du" + "liquide" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Du liquide ?

### "Excusez" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Excusez—moi !

### "Fabinho" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Oh, Fabinho !

### "Folk" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Folk - Allez, on les a.

### "Frank" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Hé, Frank !

### "Führer" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Mon Führer !

### "Gretchen Ross" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai trouvé 'Gretchen Ross' assez cool.

### "Harvard" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment êtes-vous entrées à Harvard ?

### "Humains" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ou créer des esprits libres, et les appeler 'Humains'.

### "Hurlement" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hurlement -Viens !

### "Hé" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hé, Frank !

### "Idéfix" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Au pied, Idéfix !

### "J'" + "ai" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J' ai fait un aller-retour la lune.

### "Japon" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Son ventre me rappelle les cartes postales du Japon - plat et joli.

### "Jasmeet" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Salut, Jasmeet !

### "Jim" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu savais, Jim ?

### "Jones" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - M. Jones ?

### "Jordan" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu veux me baiser, Jordan ?

### "J’" + "t’" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J’ t’assure tu devrais manger un morceau, hein.

### "Kimmie" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quatre maudites fois, Kimmie !

### "Kingsbury" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Porte de Kingsbury !

### "Kitaro" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Tenez, du porc pané de chez Kitaro ; mais pas extra.

### "L" + "‘" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - L‘homme pousse des petits cris.

### "La" + "carte" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - La carte !

### "La" + "constipation" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - La constipation ?

### "La" + "garce" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - La garce !

### "La" + "princesse" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - La princesse ?

### "La" + "solution" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - La solution ?

### "La Taverne" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - T'iras peut-être à 'La Taverne'.

### "Laissez" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Laissez -moi !

### "Lamar" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Qu'allez-vous faire, Lamar ?

### "Lance" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ça va, Lance ?

### "Le" + "corps" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Le corps ?

### "Le" + "portable" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Le portable !

### "Le" + "travail" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Le travail ?

### "Les" + "architectes" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Les architectes !

### "Lily" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ça s'appelle 'L'océan de Lily'.

### "Lui" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Lui - un salaud, elle - une sainte.

### "L’" + "«" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - L’ « Éveil » est la suprématie de la sensibilité sur la vérité.

### "M" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je croyais qu'on avait choisi 'M' au hasard.

### "M." + "Jones" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - M. Jones ?

### "Ma" + "montre" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ma montre !

### "Mademoiselle" + "Meinhof" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Mademoiselle Meinhof ?

### "Main" + "coupée" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Main coupée ?

### "Mamie" + "eeee" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Jolie Mamieeeee !

### "Marla" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Réponds, Marla !

### "Marty" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Yo, Marty !

### "Meinhof" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Mademoiselle Meinhof ?

### "Merci" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "Mexique" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Es-tu déjà allé au Mexique ?

### "Mikiya" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quoi, Mikiya ?

### "Millions" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Un Millions -Dollars ?

### "Mme" + "Powell" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Mme Powell ?

### "Mon" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oh - Mon - Dieu !

### "Mon" + "Führer" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Mon Führer !

### "Mon" + "amour" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Mon amour !

### "Mon" + "peigne" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Mon peigne !

### "Monsieur" + "--" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Regardez, Monsieur -- un droïde.

### "Mort" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et le désir me talonne et me mord, car je vous aime, ô Madame la Mort .

### "N" + "Arrête" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - NArrête de me donner des ordres !

### "N" + "‘" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - N‘importe quoi !

### "Ni" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Les chevaliers qui disent 'Ni' exigent un sacrifice.

### "O." + "C." (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Précédemment dans The O.C.

### "Oh" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oh - Mon - Dieu !

### "Ou" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ou'est-ce que c'est ?

### "Oui" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oui - dans la norme.

### "P" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ça, c’était une parade avec un «P» majuscule.

### "PK" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je suis PK .

### "Paris" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Les étudiants appellent à la révolution culturelle à Paris .

### "Partout" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Partout - bousculade, cohue, queue.

### "Passez" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Passez—moi un franc à la place de souffler comme Jurassic Park.

### "Paulo" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Allô, Paulo ?

### "Pippin" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Et toi, Pippin ?

### "Plus" + "fort" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Plus fort ?

### "Point" + "trait" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Point trait point point.

### "Polynectar" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Du Polynectar ?

### "Portable" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Portable -Excuse-moi.

### "Powell" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Mme Powell ?

### "Quel" + "bus" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quel bus ?

### "Quel" + "froid" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quel froid !

### "Quel" + "signal" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quel signal ?

### "Quel" + "sort" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quel sort ?

### "Quel" + "vacarme" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quel vacarme !

### "Quelle" + "nana" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quelle nana ?

### "Quelle" + "technique" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Quelle technique !

### "Qui" + "paie" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Qui paie ?

### "R" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - T'aurais pas un 'R' à me prêter ?

### "Raju" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Raju -Akhtar.

### "Regarde" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Regarde - Catherine !

### "Retourne" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Retourne—toi sur ton maître.

### "Rires" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rires - Merci, Éli.

### "Réponds" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Réponds, Marla !

### "Schutte" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Salut, Schutte - Pas de noms !

### "Se" + "marier" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Se marier ?

### "Se" + "pencher" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Se pencher !

### "Ses" + "collants" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ses collants ?

### "Shadow" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous m'avez dit 'Sauf pour M. Shadow'.

### "Si" + "loin" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Si loin ?

### "Son" + "frère" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Son frère ?

### "Sonnerie" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sonnerie -La cour.

### "Soupir" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Soupir - Quelle heure est-il ?

### "Sébastien" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Je veux prêter la maison à Sébastien; il veut emmener Rémy.

### "Tignasse" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Et toi, Tignasse ?

### "Tire" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tire—toi !

### "Tom" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Que fais-tu ce soir, Tom ?

### "Ton" + "peigne" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Ton peigne ?

### "Ton" + "témoin" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ton témoin ?

### "Trop" + "beau" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Trop beau !

### "Très" + "flattée" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Très flattée !

### "Très" + "profond" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Très profond !

### "Tu" + "--" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu --aptes ?

### "Un" + "agent" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Un agent ?

### "Un" + "amant" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Un amant ?

### "Un" + "meurtrier" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Un meurtrier ?

### "Un" + "partenaire" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Un partenaire ?

### "Un" + "sacrifice" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Un sacrifice ?

### "Un" + "service" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Un service ?

### "Un" + "shampoing" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Un shampoing !

### "Un" + "traître" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Un traître !

### "Une" + "amie" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Une amie ?

### "Une" + "caution" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Une caution ?

### "Une" + "douche" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Une douche ?

### "Une" + "mission" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Une mission ?

### "Une" + "ostéopathe" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Une ostéopathe ?

### "Une" + "sorcière" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Une sorcière ?

### "Urban Legends" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Notre prochaine fonctionnalité est «Urban Legends».

### "Vieux Garçon de Greer County" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'était 'Le Vieux Garçon de Greer County'.

### "Vous" + "osez" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Vous osez !

### "Workman" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Workman .

### "X" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Place un 'X' à l'endroit opportun.

### "Y" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Les hommes ont un chromosome X et un Y ; les femmes, deux X.

### "Zico" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu veux nous faire peur, Zico ?

### "^" + "iné" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sieur a^iné, je meurs de faim.

### "a" + "^" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sieur a^iné, je meurs de faim.

### "abandonnée" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Maria Goretti, seule et abandonnée; un destin de femme.

### "additionne" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Qui travaille seul additionne ; qui travaille ensemble multiplie.

### "agent" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un agent ?

### "ah" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dites 'ah'.

### "aider" + "-Aider" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ils voulaient simplement aider -Aider ?

### "ailleurs" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - C'est quoi, d'ailleurs ?

### "aller" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Non, vraiment, nous devons y aller - Venez.

### "allez" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Avel, allez - hop !

### "alors" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Qu'y a-t-il alors ?

### "alors" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ne seraient-ils pas malheureux, alors ?

### "amant" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un amant ?

### "ami" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comment dit-on 'ami' en elfe ?

### "amie" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Une amie ?

### "amour" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Mon amour !

### "amour" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Voici, tu es belle, ô mon amour ; voici, tu es belle !

### "annulée" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'est une sensation merveilleuse d'être 'annulée'.

### "apparu" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Dans quelles circonstances le défaut est-il apparu ?

### "apporté" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Qu'as-tu apporté ?

### "architectes" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Les architectes !

### "argent" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment êtes-vous venus en possession de tout cet argent ?

### "arrivé" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Ça t'est déjà arrivé ?

### "arrivée" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - À quelle heure y es-tu arrivée ?

### "as" + "senti" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - T'as senti ?

### "assez" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous dites 'assez', qu'entendez-vous par là ?

### "assidûment" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Il a étudié assidûment; autrement il aurait échoué de nouveau.

### "attends" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hé, Hubert, attends—moi là, oh !

### "au" + "millet" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Et des pains au millet ?

### "aussi" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et un faune aussi -Un faune ?

### "autres" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Toi et les autres - tous en tôle.

### "aux" + "cellules" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Tout le monde aux cellules !

### "avancent" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Les travaux avancent ?

### "avec" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ne jouez pas avec , Vous pourriez devenir aveugle.

### "avis" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Que s'est-il passé, à votre avis ?

### "avoir" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "babu" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hé Piku, ton Paanchu babu .

### "baronne" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Peut-on vous appeler autrement que 'Mme la baronne' ?

### "beau" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Trop beau !

### "berger" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Le Seigneur est mon berger ; je ne manquerai de rien.

### "bien" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Alors tout va bien .

### "bien" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Ton ordinateur ne marche pas bien ; supprime les logiciels inutiles.

### "bip" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'entends juste le 'bip' d'une balise.

### "blague" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Non, je blague; appelez moi Papi.

### "blesser" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Oui, ça va la blesser; mais il faut voir sur le long terme.

### "bleu" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Les appels qui ont été faits du 'Boeuf bleu'.

### "boire" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Boire ou ne pas boire - telle est la question.

### "bonbon" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La presse a appelé ça 'la défense bonbon'.

### "bordel" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Relève-toi, bordel !

### "bras" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rentrez vos bras , et vos mains.

### "bruyante" + "bête" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - La bruyante bête !

### "bus" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quel bus ?

### "bébé" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai dis 'oh oui bébé', viens voir.

### "bête" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La bruyante bête !

### "c" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le mieux que tu puisses faire, c'est essayer.

### "cadeau" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Alors il lui a acheté un cadeau: ce chaton.

### "calibre" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - C‘est toi qui as le calibre, non ?

### "capacités" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je commence à douter de tes capacités .

### "cardinal" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Un est un nombre cardinal ; premier est un nombre ordinal.

### "carte" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La carte !

### "caution" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Une caution ?

### "ce" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Qui est-ce , Frederick ?

### "ce" + "correct" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Est-ce correct ?

### "ce" + "voyage" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Et ce voyage ?

### "celle" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Elle est au bord de la crise cardiaque, celle—là.

### "cellules" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tout le monde aux cellules !

### "ces" + "jours" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je le prends les mardis et jeudis comme tu es en congé cesjours-là.

### "chasse" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Papa est parti à la 'chasse'.

### "chat" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ma mère sait à peine compter ou épeler 'chat'.

### "chaton" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Tom a un chien et Marie un chaton ; les deux animaux s’entendent bien.

### "chip" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je fais un 'chip' au-dessus de l'eau.

### "chose" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je crois qu'il te manque quelque chose .

### "chose" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Interrogez-vous sur chaque chose: quelle est son essence ?

### "chéri" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ne manque pas mon jeu, mon chéri - Des rubis ?

### "cinglé" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Lâche cinglé - il ne pouvait même pas mordre le foutu.

### "circonstances" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Au diable les circonstances ; je crée des opportunités.

### "collants" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ses collants ?

### "collation" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "colorer" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ce qui peut tout animer, tout adoucir, tout colorer - un grand amour.

### "colère" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Tu ne seras pas puni pour ta colère ; Tu seras puni par ta colère.

### "commercial" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Centre commercial , hein ?

### "communautaire" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Nous avons une auto-protection communautaire .

### "connaissez" + "Jeremy" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous connaissezJeremy ?

### "connection" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Nous voici dans la chambre de connection .

### "conscience" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vous ne souhaitez pas avoir ce sang sur la conscience .

### "considérable" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ce qui est considérable , je présume.

### "conspiration" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je faisais partie de la conspiration - Chien !

### "constipation" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La constipation ?

### "contrat" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Nous aimerions vous demander de reprendre son contrat .

### "corps" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Le corps ?

### "correct" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Est-ce correct ?

### "correcte" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - La phrase est correcte ; je la formulerais cependant autrement.

### "costauds" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il y a une rangée externes de colonnes, Des matériaux vraiment costauds .

### "coupée" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Main coupée ?

### "courir" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu dois courir , OK ?

### "courriel" + ">" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Envoyez-nous votre CV détaillé à <courriel>.

### "cousins" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Vous êtes des cousins ?

### "cow" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Arrête de jouer les cow—boys, papy.

### "cuillère" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Tom mange le riz à la cuillère ; Marion, en revanche, préfère les baguettes.

### "d" + "â" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et tout sera sec en un clin dâ€™œil.

### "de" + "Josie" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je voulais faire partie deJosie et les Pussycats.

### "de" + "Kingsbury" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Porte de Kingsbury !

### "de" + "jouer" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Arrêtez de jouer !

### "de" + "régénérer" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Arrête de régénérer !

### "de" + "sécurité" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Des agents de sécurité !

### "de" + "́ja" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - As-tu déjà ramassé un champignon vénéneux ?

### "des" + "cousins" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Vous êtes des cousins ?

### "dessus" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le cochon lui saute dessus - chut.

### "destin" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai un 'cube du destin' sur moi.

### "destin" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Je suis le maître de mon destin ; je suis le capitaine de mon âme.

### "deux" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment fait-on pour séparer son âme en deux, monsieur ?

### "devoir" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Moi et le mot 'devoir' dans la même phrase.

### "dignité" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le turban est ta dignité , préserve-la.

### "ding" + "eringeding" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ding-ding-ding-ding-dingeringeding !

### "douche" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Une douche ?

### "doute" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La vérité est belle, sans doute; mais de même des mensonges.

### "du" + "monde" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Le meilleur attrapeur du monde !

### "décider" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Comment décider ?

### "déjà" + "fugué" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Il a déjà fugué ?

### "effet" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le deuxième acte s'appelle 'l'effet'.

### "eh" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Eh oui, mais - eh - les autres ?

### "elle" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Lui - un salaud, elle - une sainte.

### "en" + "est" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ça en est !

### "encore" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comment ça, 'encore' ?

### "enfants" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Les femmes et les enfants ?

### "enfoiré" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Où est ce fils de pute, enfoiré ?

### "ennemis" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pardonnez toujours à vos ennemis - rien ne les agace plus que cela.

### "entrée" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment es-tu entrée ?

### "entrées" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment êtes-vous entrées ?

### "entêté" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Je ne suis pas entêté ; je suis tenace.

### "escorté" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il est toujours escorté - deux, trois chiens de garde.

### "espion" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Cet homme est un espion ; il faut qu’il meure.

### "est" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ça en est !

### "est" + "long" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Que ce pont est long !

### "et" + "je" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je souffle un instant etje prépare à manger.

### "explose" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vas-y, explose—lui sa mère !

### "expression" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et toujours la même expression, hein ?

### "faim" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Il ne peut pas avoir faim ; il vient de déjeuner.

### "faire" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu me demandes pas ce que je vais faire ?

### "fait" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Elle veut savoir ce qu'elle a fait .

### "fait" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Il s'est excusé, mais le mal était fait; après, c'était trop tard.

### "faut" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - On l'appelle le 'jeune homme comme il faut'.

### "feu" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dès que prêts, feu .

### "file" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Suivez la file .

### "fille" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Vous aimez ma fille ; mais êtes-vous sûr qu’elle vous aime ?

### "filles" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Surveille les filles ; elles ne savent pas nager.

### "film" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le film .

### "flattée" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Très flattée !

### "fleurs" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Abeilles cruelles, suçant toute vie de ces pauvres fleurs .

### "foire" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Nous allons à la foire ; viens-tu ?

### "fort" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Pas trop fort !

### "fort" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Plus fort ?

### "fortune" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et ça vaut une fortune .

### "fourvoyé" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Pourquoi me suis-je fourvoyé ?

### "frites" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Des frites ?

### "froid" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quel froid !

### "frère" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Son frère ?

### "fugué" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Il a déjà fugué ?

### "fête" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Combien de temps êtes-vous restés à la fête ?

### "fête" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Cette salope de la fête ?

### "garce" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La garce !

### "garde du corps" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu sais pas écrire 'garde du corps' ?

### "gars" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Par 'ces gars' vous voulez dire mes clients ?

### "gauche" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tournez à gauche quand je dis 'gauche'.

### "gauche" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Droite ou gauche ?

### "gentillesse" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mais votre 'gentillesse', elle nous rend petits comme ça.

### "girafe" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je passe après 'tasse', 'piscine' et 'girafe'.

### "gombo" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Le bouc fait le gombo ; le lapin le mange.

### "gouvernement" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ensuite, qu'est-ce qui constitue le 'meilleur gouvernement' ?

### "grec" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Ils sont en grec ; on ne peut pas les lire.

### "génotype" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et comme vous avez le même génotype , vous pourriez enfiler ses chaussures.

### "général" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Pourquoi ne commencez-vous pas, général ?

### "hier" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Où sont-ils allés hier ?

### "homme" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dites juste 'conséquences pour la santé de l'homme'.

### "honnête" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Elle est brutalement honnête ; ça blesse de temps à autre.

### "horrible" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ah, pas spécialement horrible; non, non, pas du tout.

### "ici" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Les seuls criminels portent les uniformes nazis, ici !

### "ici" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment êtes-vous parvenues ici ?

### "idiot" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Suis-je idiot ?

### "ingrédients" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il faut neuf ingrédients - Dis nous !

### "inspecteur" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sur ma plaque, il y a 'inspecteur'.

### "intentions" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Alors, tu disais 'meilleures intentions' ?

### "international" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - On va devoir se servir du 'langage international'.

### "intéressant" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - C’est intéressant, non ?

### "investissement" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Votre frère représentait un immense investissement .

### "irlandaise" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Si on donnait une touche 'irlandaise' à ces cafés ?

### "irrésistible" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Oui, on a appelé ça aussi 'impulsion irrésistible'.

### "italiens" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Nous sommes italiens ; nous utilisons toujours nos mains.

### "j" + "‘" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu crois, j‘ai que de la gueule ?

### "joie" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cris de joie - Bravo.

### "jouer" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Arrêtez de jouer !

### "jour" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Seize heures font un jour ; huit heures font une nuit.

### "juifs" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Alors tu es 'Le chasseur de juifs'.

### "klaxon" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Coups de klaxon - Pardon.

### "la" + "fête" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Cette salope de la fête ?

### "la" + "part" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - De la part ?

### "la" + "police" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - De la police ?

### "lac" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - De la pelouse vers le lac - oui ?

### "larmes" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Éthel versait des larmes ; le jeune homme se précipita à ses pieds.

### "le" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Débranchez le .

### "leader" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ici leader , nous entrons dans le courant du vortex.

### "lei" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Il y a trois ans, je payais quatre-vingts lei ; maintenant je paie deux cents.

### "les" + "enfants" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Les femmes et les enfants ?

### "leur" + "temps" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Autrement, ils passeraient leurtemps ici.

### "lien" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le premier vol scelle le lien , tu ne peux attendre.

### "liquide" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Du liquide ?

### "lis" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Je lis; tu écris.

### "loin" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Si loin ?

### "long" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Que ce pont est long !

### "lui" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Non, jamais entendu parler de lui .

### "lui" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Qui, lui ?

### "là" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vous creusez là - tiens donc, le noyau atomique est constitué de protons.

### "là" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Notre voiture est là ; où est la tienne ?

### "là" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Erina est là; il faut que je sorte.

### "là" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu vois le barbu, là ?

### "là" + "aussi" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Celle-là aussi ?

### "lèche-vitrines" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Avec qui êtes-vous allés faire du lèche-vitrines ?

### "m" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et là, il dit : 'Tape-m'en quatre.'

### "ma" + "main" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Lâche ma main !

### "main" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Lâche ma main !

### "maintenant" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - As-tu du temps, maintenant ?

### "mais" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il y a un 'mais'.

### "mais" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Eh oui, mais - eh - les autres ?

### "maison" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sûrement pas la maison, non !

### "maman" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Pas vrai, maman ?

### "mammifère" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Une baleine est un mammifère ; autrement dit, elle allaite ses petits.

### "marchera" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ça marchera ?

### "marier" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Matthew va bientôt se marier ; il fera un très beau marié.

### "marier" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Se marier ?

### "mariée" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Quand vous êtes-vous mariée ?

### "mascarade" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je préfère un bon combat à toute cette mascarade .

### "matin" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - À quelle heure vous êtes-vous réveillés ce matin ?

### "maxi" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sous une maxi .

### "maîtres" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Un chien a des maîtres ; un chat, des serviteurs.

### "mec" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Que foutent ces mines ici, mec ?

### "merci" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je voudrais dire «merci» à Tom.

### "merveilleuse" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu es merveilleuse, simplement merveilleuse -Quelle sublime robe.

### "meurtrier" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un meurtrier ?

### "millet" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Et des pains au millet ?

### "mission" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Une mission ?

### "moi" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il sait que c'est votre autre 'moi'.

### "moi" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu as besoin de moi , Guido ?

### "moi" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Le livre est pour moi; les fleurs sont pour nous.

### "mon" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "monde" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Le meilleur attrapeur du monde !

### "monde" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dehors il y a le vrai monde , et le rêve ici.

### "monde" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ca va être un nouveau départ dans un nouveau monde .

### "monde" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - La physique ne décrit pas le monde ; elle décrit ce que nous savons du monde.

### "montre" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ma montre !

### "moustiques" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Des moustiques ?

### "même" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Regarde-toi toi-même, épouvantail !

### "nana" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai 'fait une nana' un tas de fois.

### "nana" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quelle nana ?

### "neige" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ça allait devenir 'L'année de la grande neige'.

### "nique" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ah oui, nique—moi ?

### "niquer" + "ons" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dimanche prochain, nous pique-niquerons.

### "nmiolomhobbit pa" + "rler." (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Laisse le… canmiolomhobbit parler.

### "non" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Sûrement pas la maison, non !

### "non" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Malheureusement non ; au contraire.

### "nourrir" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - J’ai une famille moi, ça me fait des bouches à nourrir .

### "nous" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ils s'amènent toujours la nuit, et nous, quoi ?

### "nous" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bienvenue à Pandora, content de vous avoir parmi nous .

### "nous" + "jouer" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sommes-nous en colloque ou allons-nousjouer au golf ?

### "nu" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qu'entendez-vous par 'nu'.

### "nuit" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Oui, c'est une grande nuit; vous avez raison.

### "n’" + "y" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dans le monde, il n’ y a que quatre parfums masculins de base.

### "objets" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mes nanas sont des 'femmes objets', d'après elle.

### "obligée" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous n'y avez pas été 'obligée', non ?

### "ongles" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pendant que Cathy se fait les ongles .

### "ordinaire" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Que faites-vous chez toi, d’ordinaire ?

### "osez" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Vous osez !

### "ostéopathe" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Une ostéopathe ?

### "ou" + "gauche" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Droite ou gauche ?

### "oui" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mon opinion professionelle, oui , monsieur.

### "où" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je parle au capitaine 'je-ne-sais-d'où'.

### "pacte" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La première partie s'appelle 'le pacte'.

### "pactole" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Visez moi ce gros pactole .

### "paie" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Qui paie ?

### "panne" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Notre voiture est tombée en panne ; on a donc dû appeler un taxi.

### "parlent" + "…" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Elles vous parlent …euh…d’amour ?

### "parler" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu te rappelles qu'on a parlé de 'parler' ?

### "parlé" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu lui as vraiment parlé ?

### "part" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - De la part ?

### "partenaire" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un partenaire ?

### "partie" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Quand est-elle partie ?

### "pas" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ne vous arrétez pas , allez y !

### "pas" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Avec de tels pouvoirs, ça ne rigole pas - Salut, Klaus.

### "pas" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ne tirez pas .

### "pas" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Ceux qui savent ne parlent pas ; ceux qui parlent ne savent pas.

### "passé" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Alors, que s'est-il passé ?

### "patrie" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - On croit mourir pour la patrie; on meurt pour les industriels.

### "peigne" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Mon peigne !

### "peigne" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ton peigne ?

### "pencher" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Se pencher !

### "percée" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mon frère, je vais faire une percée , suis moi derrière.

### "peu" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La pousser un peu , vous voyez ?

### "pieds" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Prenez garde où vous mettez les pieds ; le sol s’affaisse par endroits.

### "pire" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Un meurtrier et pire - pas même de compassion pour cette malheureuse.

### "piscine" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je passe après 'tasse', 'piscine' et 'girafe'.

### "plaint" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Qui se plaint ?

### "plan" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Quel est le but central de ce plan ?

### "planque" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comment dit-on 'planque' en français ?

### "plat" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il est stable quand il est maintenu à plat .

### "point" + "point" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Point trait point point.

### "police" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - De la police ?

### "portable" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Le portable !

### "porte" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il ouvre la porte , OK ?

### "possible" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous avez oublié deux mots très importants 'si possible'.

### "pote" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je te fais une remise 'vieux pote'.

### "potes" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Entre nous, on s'appelait 'les potes'.

### "pouce" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le pouce .

### "pour" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "pour" + "jolie" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Joli muffin pourjolie madame.

### "pourriture" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Regarde-moi dans les yeux, pourriture !

### "pouvoir" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le temps n’est pas un «pouvoir».

### "premier" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu parles en premier; je parlerai ensuite.

### "princesse" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La princesse ?

### "problème" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pas de problème - vous les évacuez.

### "profond" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Très profond !

### "promis" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je lui revaudrai ça l'an prochain, promis .

### "publié" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Quand ton livre sera-t-il publié ?

### "puis" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et puis , tout a changé.

### "puissant" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dieu tout-puissant , qui sait tout.

### "putain" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Arrêtez la voiture putain .

### "pute" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Où est ce fils de pute, enfoiré ?

### "qu" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Un bonjour sincère vaut mieux qu'un long discours.

### "que" + "je" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu veux queje vienne ?

### "que" + "tout" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Plus que tout ?

### "quoi" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ils s'amènent toujours la nuit, et nous, quoi ?

### "rabbit" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je viens d'acheter une pièce de la 'rabbit'.

### "raison" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quand j'ai dit 'j'ai toujours raison' ?

### "raison" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La guerre ne décide pas qui a raison - seulement qui est vivant.

### "rappel" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai fait 'rappel' et c'était toi.

### "recevrez" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Demandez et vous recevrez ; votre joie sera parfaite.

### "recule" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tanjiro, recule !

### "refuser" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Alors pourquoi refuser ?

### "rencontrés" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment vous êtes-vous tous deux rencontrés ?

### "rendez" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dans mon quartier, il faut prendre rendez—vous.

### "rendez-vous" + "ici" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Pourquoi ce rendez-vous ici ?

### "rentré" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment es-tu rentré ?

### "repose" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mme Delassalle se repose - Entrez donc !

### "ressemble" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu leur ressemble , tu parles comme eux et bientôt ils nous feront confiance.

### "rester" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Courir , rester , alors quoi ?

### "retard" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Désolé, je suis en retard - Mais marche alors.

### "rien" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Mais nous ne savons vraiment rien ; car la vérité se trouve tout au fond.

### "rler." + "" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Laisse le… canmiolomhobbit parler.

### "roi" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'est toi qui as dit 'roi'.

### "rouge" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et son pseudo-jargon militaire style 'Code rouge'.

### "russo" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Touristo russo - On est la morale infuse !

### "régénérer" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Arrête de régénérer !

### "réincarné" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - En quoi serez-vous réincarné ?

### "réponse" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Comment êtes-vous parvenus à cette réponse ?

### "rêves" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - On lui a raconté des choses, elle a fait des rêves - elle a peur.

### "sacrifice" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un sacrifice ?

### "sais" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je sais .

### "savoir-faire" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'est quoi une 'marque de savoir-faire' ?

### "scientifiques" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Encore des trucs merdiques de scientifiques .

### "se" + "plaint" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Qui se plaint ?

### "senti" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - T'as senti ?

### "serait" + "jamais" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sans lui on se seraitjamais connus.

### "serveurs" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Le bar Zailaiba embauche des serveurs ; es-tu intéressé ?

### "service" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un service ?

### "shampoing" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un shampoing !

### "signal" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quel signal ?

### "signatures" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Des signatures ?

### "sincère" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Elle est trop sincère; parfois ça me blesse.

### "sismique" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Alerte sismique - Bon sang !

### "soi" + "--" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Chacun pour soi -- Dieu pour tous.

### "soir" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Vous êtes-vous amusées, ce soir ?

### "soldat" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quand on monte l'échelle, on trouve le 'soldat'.

### "solution" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La solution ?

### "sont" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Où sont -ils ?

### "sorcière" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Une sorcière ?

### "sort" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quel sort ?

### "soudaine" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ton rire est une vague argentée soudaine .

### "souhaits" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Des souhaits ?

### "sourit" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - La jeune fille se tut et sourit ; le jeune homme se tut et soupira.

### "suis" + "--" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je suis -- Douglas Kelley.

### "suivant" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Plateau suivant .

### "survivront" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Les forts survivront ; les faibles périront.

### "sécurité" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Des agents de sécurité !

### "séparions" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Il faut que nous nous séparions ; le jour ne va pas tarder à paraître.

### "sûrs" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il y aura une réunion avec des gens 'sûrs'.

### "t" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et vers où compte-t'il la faire marcher ?

### "table" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Vous prenez une table ?

### "tapette" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le dernier mot qu'il a entendu était 'tapette'.

### "tasse" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je passe après 'tasse', 'piscine' et 'girafe'.

### "technique" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quelle technique !

### "temps" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Si vous le désirez, avec le temps , vous serez en mesure de lui pardonner.

### "terre" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vous venez de la terre .

### "test" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qu'est-ce que tu veux dire par 'test' ?

### "tire" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vas-y, tire—lui une balle, si tu veux.

### "toi" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sauf que 'toi' ne veut plus rien dire.

### "toi" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Souviens-toi: ouvre le avant de le manger.

### "toi" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Je ne priais pas contre toi ; je priais pour toi.

### "toi" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Comment tu sais ça, toi ?

### "tomber" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La Catalogne est sur le point de tomber .

### "toujours" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Elle vous écoutait toujours .

### "tout" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Plus que tout ?

### "trait" + "point" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Point trait point point.

### "traites" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Traites, pas traites - sortez-les !

### "transformation" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Que vouliez-vous dire par le terme 'transformation', docteur ?

### "transparence" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - La viscosité de ses sécrétions et leur transparence ?

### "traquer" + "Jabba" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Nous allons traquerJabba et le chasseur de primes.

### "travail" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Le travail ?

### "travailler" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vous avez été amené ici pour travailler - alors travaillez.

### "travaux" + "avancent" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Les travaux avancent ?

### "traître" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Un traître !

### "trompe" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Jamais la nature ne nous trompe ; c’est toujours nous qui nous trompons.

### "trop" + "fort" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Pas trop fort !

### "trouve" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Cherche ; trouve ; découvre !

### "truands" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Des truands ?

### "tu" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Que pensais-tu ?

### "tu" + "apporté" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Qu'as-tu apporté ?

### "tuer" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ils sont très difficiles à tuer .

### "témoin" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Ton témoin ?

### "underground" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - En Grande-Bretagne le métro se nomme « underground» et non « subway ».

### "une" + "table" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Vous prenez une table ?

### "utile" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Les livres sont vieux, mais très utile .

### "va" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Comment va ?

### "va" + "Charlie" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comment ça vaCharlie ?

### "vacances" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Vous êtes-vous bien amusés pendant vos vacances ?

### "vacarme" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Quel vacarme !

### "vais" + "faire" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Tu me demandes pas ce que je vais faire ?

### "vas" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quoi, vas—y ?

### "vide" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Détendez vous et faites le vide .

### "ville" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Êtes-vous arrivées en ville ?

### "vite" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Aussi vite ?

### "vivre" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - En espagnol, ça veut dire 'Je te laisse vivre'.

### "voici" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Natacha, Véra, le voici - Dénissov.

### "vois" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Là, je mettrai le fenouil, le persil, tu vois - directement sur la table.

### "voiture" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous m'avez dit 'un accident de voiture'.

### "voler" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Vivre signifie séduire et voler; tourbilloner et rayonner.

### "votre" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Voici le votre .

### "vous" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Souvenez vous , des endroits comme le site que vous venez juste de détruire.

### "vous" + "…" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ce qui fait de vous … un moins que rien.

### "voyage" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Et ce voyage ?

### "vraiment" + "parlé" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Tu lui as vraiment parlé ?

### "y" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Equipe mech allez y .

### "zone" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quoi d'autre, 'hors de la zone' ?

### "«" + "Docteur" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ils me donnèrent du « Docteur ».

### "«" + "Et" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je le lui ai dit, et il a répondu « Et alors ?

### "«" + "Fonds Monétaire International" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - FMI signifie « Fonds Monétaire International ».

### "«" + "Ha" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La première exclamation de John était, « Ha-ha !

### "«" + "Hummers" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Les « Hummers » sont des gouffres à carburant.

### "«" + "Le" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Nous appelons cette ville « Le Petit Kyoto ».

### "«" + "Les" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Les enfants vont maintenant chanter « Les quatre Questions ».

### "«" + "Mistral" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le vent qui souffle souvent en Provence s’appelle le « Mistral ».

### "«" + "Natto" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le « Natto » a une odeur terrible, mais un goût délicieux.

### "«" + "Opération" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - L’« Opération dîner pour l’agent Wong » ?

### "«" + "Poètes" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je viens tout juste de devenir président de « Poètes sans maisons ».

### "«" + "Quoi" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment ça, « Quoi ?

### "«" + "Strauss" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ca se prononce « Strauss ».

### "«" + "Tom" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - On dit que « Tom et Marie » est le roman le plus ennuyeux au monde.

### "«" + "Tomei" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La voie rapide « Tomei » relie Tokyo à Nagoya.

### "«" + "Twitter" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - On peut à présent suivre le pape sur « Twitter ».

### "«" + "ah" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Puis dites « ah ».

### "«" + "blagues" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rien de neuf dans la rubrique « blagues » ?

### "«" + "brillant" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Que vous êtes un « brillant spécialiste des têtes ».

### "«" + "bœuf" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le pluriel de « bœuf » est « bœufs ».

### "«" + "bœufs" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le pluriel de « bœuf » est « bœufs ».

### "«" + "homard" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment dis-tu « homard » en français ?

### "«" + "hommage" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Quel est le mot juste pour « hommage », Sharon ?

### "«" + "ici" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Quel est simplement le mystère de ce lieu appelé « ici » ?

### "«" + "idiot" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il y’a marqué « idiot » sur mon front ?

### "«" + "lobster" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment dis-tu « lobster » en français ?

### "«" + "merci" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu devrais au moins dire « merci ».

### "«" + "mon" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ziri appelle Rima « mon sucre ».

### "«" + "muscle cars" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tom aime les « muscle cars ».

### "«" + "oui" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dis « oui », s’il te plaît !

### "«" + "pause" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il a mis le film sur « pause » et est allé aux toilettes.

### "«" + "pretty" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment écris-tu « pretty » ?

### "«" + "salut" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - As-tu dit « salut » ?

### "«" + "satisfaisant" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pourquoi ai-je obtenu un « satisfaisant » ?

### "«" + "science" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hitler qualifiait la physique quantique de « science juive ».

### "«" + "subway" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - En Grande-Bretagne le métro se nomme « underground» et non « subway ».

### "«" + "suffisant" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tom a eu un « suffisant » à son contrôle continu.

### "«" + "syndrome" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - SSPT signifie « syndrome de stress post-traumatique ».

### "«" + "synonyme" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Y a t-il un autre mot pour « synonyme » ?

### "«" + "tant" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Que voulez-vous dire par « tant de personnes » ?

### "«" + "thé" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ce thé est appelé « thé vert ».

### "«" + "travailler" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu connais ce mot, « travailler » ?

### "«" + "trobairitz" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Une femme troubadour est appelée communément « trobairitz ».

### "«" + "trouble" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - TSPT signifie « trouble de stress post-traumatique ».

### "«" + "underground" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - En Grande-Bretagne le métro se nomme « underground» et non « subway ».

### "«" + "Éveil" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - L’ « Éveil » est la suprématie de la sensibilité sur la vérité.

### "«" + "état" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - ESPT signifie « état de stress post-traumatique ».

### "Ça" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ça - le volume.

### "Ça" + "aussi" (1 occurrences)
- Predicted: Space
- Actual: Nbsp
- Examples:
  - Ça aussi ?

### "Ça" + "marchera" (1 occurrences)
- Predicted: Space
- Actual: NarrowNbsp
- Examples:
  - Ça marchera ?

### "â" + "€™œil" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et tout sera sec en un clin dâ€™œil.

### "ça" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Elle a toujours été comme ça - Ne dis pas des choses pareilles.

### "école" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Pour le journal de l'école ?

### "épouvantail" + "!" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Regarde-toi toi-même, épouvantail !

### "équipe" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Votre équipe - là-bas, derrière la tombe.

### "éteintes" + "?" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Nbsp
- Examples:
  - Pourquoi les lumières se sont-elles éteintes ?

### "évidemment" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Il est très mal soigné, évidemment; il commence à souffrir beaucoup.

### "île" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'est 'île' en espagnol mais à l'envers.

### "погода" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Каждая погода - благодать.

### "–" + "vous" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Asseyez–vous.

### "—" + "boys" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Arrête de jouer les cow—boys, papy.

### "—" + "là" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Elle est au bord de la crise cardiaque, celle—là.

### "—" + "vous" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dans mon quartier, il faut prendre rendez—vous.

### "—" + "y" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quoi, vas—y ?

### "…" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Maman… -On ne discute pas.

### "…" + "Mais" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Voilà…Mais là, je suis calme.

### "…" + "allez" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bien…allez-y.

### "…" + "assis" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hier vous…assis !

### "…" + "de" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - De quoi…de champignons.

### "…" + "d’" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Elles vous parlent …euh…d’amour ?

### "…" + "et" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - A… alors cachons le… Et…et où… ?

### "…" + "euh" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Elles vous parlent …euh…d’amour ?

### "…" + "je" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je…je…vous plains sincèrement.

### "…" + "mais" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Non…mais on ne peut pas trouver autre chose que de la séduire ?

### "…" + "oui" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - O…oui.

### "…" + "une" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Traite la comme…une femme.

### "…" + "voilà" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Servez-vous…voilà, des tomates.

### "…" + "vous" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je…je…vous plains sincèrement.

### "…" + "–" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dori, Nori… – Pas mes tomates de concours !

### "… ca" + "nmiolomhobbit pa" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Laisse le… canmiolomhobbit parler.

### "………" + "cinq" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Trente………cinq !

