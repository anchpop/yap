# Whitespace Prediction Diagnostics

Total predictions: 2668595
Total errors: 634
Accuracy: 99.98%

## Error Patterns (sorted by frequency)

### "'" + "?" (20 occurrences)
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

### "qu" + "‘" (8 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et qu‘on ne les y reprenne plus.
  - Il paraît qu‘un keuf a perdu son calibre dans la cité, cette nuit.
  - Je connais pas le keuf qu‘a perdu le flingue.

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

### "," + "non" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Non,non,non.
  - Non,non,non.
  - Non,non.

### "," + "que" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Allez-vous chanter la chanson,que vous chantiez chaque année ?
  - Bonjour,que voulez-vous ?
  - Chikku,que fais-tu ici ?

### "," + "«" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment ça, « Quoi ?
  - La première exclamation de John était, « Ha-ha !
  - Tu connais ce mot, « travailler » ?

### "--" + "Je" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Donc, de toute façon, je-- Je ne sais pas.
  - Il a dit que la philosophie-- Je frime, donc arrête-moi.
  - J'étais marié avec-- Je peux m'asseoir là ?

### "Bips" + "-" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bips -Alerte torpille !
  - Bips -Allez, dérobe !
  - Bips -Missile ?

### "Qu" + "‘" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qu‘est—ce tu cherches ?
  - Qu‘est—ce tu fais avec ton grenaille ?
  - Qu‘est—ce tu peux faire de mieux ?

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

### "," + "en" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ne poussez pas,vous allez me foutre,en dehors de la carriole !
  - Nous sommes des aviateurs anglais,en fuite !

### "," + "tu" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Donc,tu es astronaute ?
  - Gita,tu es méchante Où est le seau ?

### "," + "vite" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ca y est, ils sont au-dessus,vite !
  - Donnez-moi une citrouille,vite !

### "-" + "le" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Il y a trop de machisme en tauromachie, reconnaissez- le.
  - Ça - le volume.

### "-" + "t" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment mon garçon a- t-il mérité cela ?
  - Vieil homme, où le Christ a- t-il été crucifié ?

### "-" + "un" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ce qui peut tout animer, tout adoucir, tout colorer - un grand amour.
  - Lui - un salaud, elle - une sainte.

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

### "Rire" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rire -Gros dossier.
  - Rire -Oui.

### "bien" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Eh bien , ils sont géants !
  - Très bien , regardez autour de vous.

### "cheese" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dis 'cheese'.
  - Dites 'cheese'.

### "i" + "I" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mais qu'iI finisse ce qu'iI a commencé.
  - Mais qu'iI finisse ce qu'iI a commencé.

### "il" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ce n'est pas : 'Où est-il' ?
  - Qui 'il' ?

### "là" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oui, et tu ne peux pas conduire le caravan là -Alors ?
  - Vous creusez là - tiens donc, le noyau atomique est constitué de protons.

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

### "monde" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dehors il y a le vrai monde , et le rêve ici.
  - Très bien tout le monde ,faisons le ménage.

### "non" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hmm non , je te manquerais.
  - Non , non , non !

### "plaît" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dis 's'il te plaît'.
  - Tu as dit 's'il te plaît' ?

### "scientifique" + "," (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je suis scientifique , vous vous en souvenez ?
  - Tommy était le scientifique , pas moi.

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

### "," + "ALLEZ" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ne perdons pas de temps, en bavardages,ALLEZ en avant, allons !

### "," + "Jeremy" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hé,Jeremy !

### "," + "John" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bonne nuit,John.

### "," + "Là" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - REGARDEZ,Là-HAUT !

### "," + "Monsieur" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je peux m arrêter,Monsieur ?

### "," + "Ndes" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il fait des maisons, des motos,Ndes ponts, des villes, des fusées.

### "," + "Nou" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comme des découvertes,Nou des créations.

### "," + "Ntu" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quand tu arrives dans un virage,Ntu dois laisser aller.

### "," + "O" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Avance-toi,O voyageur !

### "," + "OK" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il faut refaire le matériel de campagne entièrement,OK ?

### "," + "Tom" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Généralement,Tom porte des lunettes seulement quand il lit.

### "," + "Vidya" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Excellent,Vidya !

### "," + "Wanna" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Les Harkonnen détiennent ma femme,Wanna.

### "," + "bong" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous avez la langue bien pendue,bong sang !

### "," + "c" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Monsieur,c,est Melaram !

### "," + "celui" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il va pas me faire une dépression,celui-là ?

### "," + "commandotore" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Salut,commandotore !

### "," + "de" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous avez eu du cran, petit,de venir au milieu de cette jungle.

### "," + "dépêchez" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Allez,dépêchez-vous !

### "," + "elle" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et elle donc,elle a été merveilleuse !

### "," + "est" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Monsieur,c,est Melaram !

### "," + "et" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je lui procure des vêtements,et on se retrouve chez mon grand-père !

### "," + "euh" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bon,euh.

### "," + "faisons" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Très bien tout le monde ,faisons le ménage.

### "," + "fils" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous allez apprendre comment on grille un porc,fils de putes.

### "," + "hé" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Attendez-moi,hé !

### "," + "il" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Par ici,il est pas là ?

### "," + "je" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et moi,je le voyais à la mienne.

### "," + "mes" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Super,mes enfants !

### "," + "on" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Si on reste ici,on va se faire prendre !

### "," + "par" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La répétition est interrompue,par la force des mitraillettes,voilà !

### "," + "pourquoi" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Surement,pourquoi pas ?

### "," + "président" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pardon,président.

### "," + "quelle" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - En tout cas,quelle filme vont-ils montrer ?

### "," + "rien" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Non, non, non,rien.

### "," + "si" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Maintenant,si tu me permets, je veux faire la toilette bien sûr monsieur.

### "," + "sir" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Than you so much,sir !

### "," + "the" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Oh my God,the good wine.

### "," + "tout" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La région est quadrillée,tout est bouclé.

### "," + "venez" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Venez,venez !

### "," + "voilà" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La répétition est interrompue,par la force des mitraillettes,voilà !

### "," + "vous" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ne poussez pas,vous allez me foutre,en dehors de la carriole !

### "," + "à" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous savez,à avoir un boulot ici.

### "," + "ça" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sully,ça fait comment de trahir sa propre espèce ?

### "-" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et un-- -Je suis désolé, Mme Tanner.

### "-" + "Allez" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Folk - Allez, on les a.

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

### "-" + "Es" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Quoi-- Es-tu fâchée ?

### "-" + "Merci" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rires - Merci, Éli.

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

### "-" + "Salut" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Avec de tels pouvoirs, ça ne rigole pas - Salut, Klaus.

### "-" + "avoir" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "-" + "ce" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Est- ce que tu veux regarder un film ?

### "-" + "chut" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le cochon lui saute dessus - chut.

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

### "-" + "elle" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Comment va-t- elle ?

### "-" + "gardé" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

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

### "-" + "plat" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Son ventre me rappelle les cartes postales du Japon - plat et joli.

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
  - Chevtchouk, Kintchev, Nastia - tous.

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

### "-" + "vous" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pas de problème - vous les évacuez.

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

### "--" + "qui" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Leur pouvoir vient de leur leader-- qui est en quelque sorte leur cerveau.

### "/" + "Allemandes" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Friction dans les relations Allemandes/Allemandes.

### "/" + "nous" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vos publicités/nous rappellent que les diamants sont éternels.

### "/" + "ou" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et/ou appréciez-vous la compagnie des femmes ?

### "<" + "courriel" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Envoyez-nous votre CV détaillé à <courriel>.

### "Agitation" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Agitation -Du calme !

### "Allemandes" + "/" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Friction dans les relations Allemandes/Allemandes.

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

### "Bonjour" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bonjour -Bonjour, bébé !

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

### "Et" + "/" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et/ou appréciez-vous la compagnie des femmes ?

### "Excusez" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Excusez—moi !

### "FBI" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pazzi, le FBI -Je ne suis pas ici.

### "Folk" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Folk - Allez, on les a.

### "Gretchen Ross" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai trouvé 'Gretchen Ross' assez cool.

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

### "Mamie" + "eeee" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Jolie Mamieeeee !

### "Merci" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "Millions" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Un Millions -Dollars ?

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

### "Nastia" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Chevtchouk, Kintchev, Nastia - tous.

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

### "Omaticaya" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je suis tombé amoureux de la forêt, du peuple Omaticaya .

### "Ou" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ou'est-ce que c'est ?

### "P" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ça, c’était une parade avec un «P» majuscule.

### "Paris" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Les étudiants appellent à la révolution culturelle à Paris .

### "Passez" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Passez—moi un franc à la place de souffler comme Jurassic Park.

### "Point" + "trait" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Point trait point point.

### "Portable" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Portable -Excuse-moi.

### "R" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - T'aurais pas un 'R' à me prêter ?

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

### "Schutte" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Salut, Schutte - Pas de noms !

### "Shadow" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vous m'avez dit 'Sauf pour M. Shadow'.

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

### "Tire" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tire—toi !

### "Vieux Garçon de Greer County" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'était 'Le Vieux Garçon de Greer County'.

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

### "accord" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bon, d’accord , Eddie.

### "additionne" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Qui travaille seul additionne ; qui travaille ensemble multiplie.

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

### "ami" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comment dit-on 'ami' en elfe ?

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

### "aussi" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et un faune aussi -Un faune ?

### "avec" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ne jouez pas avec , Vous pourriez devenir aveugle.

### "avoir" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Merci - avoir - gardé collation - pour - mon - officier !

### "baronne" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Peut-on vous appeler autrement que 'Mme la baronne' ?

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

### "bras" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rentrez vos bras , et vos mains.

### "bébé" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai dis 'oh oui bébé', viens voir.

### "cadeau" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Alors il lui a acheté un cadeau: ce chaton.

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

### "ce" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Qui est-ce , Frederick ?

### "celle" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Elle est au bord de la crise cardiaque, celle—là.

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

### "circonstances" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Au diable les circonstances ; je crée des opportunités.

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

### "contrat" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Nous aimerions vous demander de reprendre son contrat .

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

### "de" + "Jésus-Christ" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Une armée deJésus-Christ portant sa sainte croix ne peut être battue.

### "de" + "́ja" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - As-tu déjà ramassé un champignon vénéneux ?

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

### "devoir" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Moi et le mot 'devoir' dans la même phrase.

### "doute" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - La vérité est belle, sans doute; mais de même des mensonges.

### "dé" + "Jà" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu as déJà entendu ça, non ?

### "effet" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le deuxième acte s'appelle 'l'effet'.

### "elle" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Lui - un salaud, elle - une sainte.

### "encore" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comment ça, 'encore' ?

### "ennemis" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pardonnez toujours à vos ennemis - rien ne les agace plus que cela.

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

### "faim" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Il ne peut pas avoir faim ; il vient de déjeuner.

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

### "fortune" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et ça vaut une fortune .

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

### "ils" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qui 'ils' ?

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

### "lien" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Le premier vol scelle le lien , tu ne peux attendre.

### "lis" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Je lis; tu écris.

### "lui" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Non, jamais entendu parler de lui .

### "là" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Notre voiture est là ; où est la tienne ?

### "m" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et là, il dit : 'Tape-m'en quatre.'

### "mais" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Il y a un 'mais'.

### "mammifère" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Une baleine est un mammifère ; autrement dit, elle allaite ses petits.

### "marier" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Matthew va bientôt se marier ; il fera un très beau marié.

### "maîtres" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Un chien a des maîtres ; un chat, des serviteurs.

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

### "nana" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - J'ai 'fait une nana' un tas de fois.

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

### "non" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Malheureusement non ; au contraire.

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

### "on" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Qui 'on' ?

### "ongles" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pendant que Cathy se fait les ongles .

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

### "panne" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Notre voiture est tombée en panne ; on a donc dû appeler un taxi.

### "parler" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu te rappelles qu'on a parlé de 'parler' ?

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

### "patrie" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - On croit mourir pour la patrie; on meurt pour les industriels.

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

### "piscine" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Je passe après 'tasse', 'piscine' et 'girafe'.

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

### "premier" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Tu parles en premier; je parlerai ensuite.

### "problème" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pas de problème - vous les évacuez.

### "promis" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Je lui revaudrai ça l'an prochain, promis .

### "publicités" + "/" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vos publicités/nous rappellent que les diamants sont éternels.

### "puis" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Et puis , tout a changé.

### "putain" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Arrêtez la voiture putain .

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

### "rendez" + "—" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dans mon quartier, il faut prendre rendez—vous.

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

### "sincère" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: None
- Examples:
  - Elle est trop sincère; parfois ça me blesse.

### "soi" + "--" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Chacun pour soi -- Dieu pour tous.

### "soldat" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quand on monte l'échelle, on trouve le 'soldat'.

### "sont" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Où sont -ils ?

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

### "trompe" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Jamais la nature ne nous trompe ; c’est toujours nous qui nous trompons.

### "trouve" + ";" (1 occurrences)
- Predicted: NarrowNbsp
- Actual: Space
- Examples:
  - Cherche ; trouve ; découvre !

### "tuer" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ils sont très difficiles à tuer .

### "underground" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - En Grande-Bretagne le métro se nomme « underground» et non « subway ».

### "va" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - On y va, on y va , on y va !

### "va" + "Charlie" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Comment ça vaCharlie ?

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

### "vivre" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - En espagnol, ça veut dire 'Je te laisse vivre'.

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

### "«" + "ici" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Quel est simplement le mystère de ce lieu appelé « ici » ?

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

### "â" + "€™œil" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Et tout sera sec en un clin dâ€™œil.

### "île" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - C'est 'île' en espagnol mais à l'envers.

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

