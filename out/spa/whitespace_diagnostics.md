# Whitespace Prediction Diagnostics

Total predictions: 2423149
Total errors: 747
Accuracy: 99.97%

## Error Patterns (sorted by frequency)

### "," + "?" (35 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - A ducharme, tendremos que estar presentables, ?
  - Angela, ?
  - Angela, no quiero enganarte, ?

### "," + "¿" (20 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,esa es la pregunta que plantea la película,¿no?
  - Bueno,¿por qué lo preguntas?
  - Bueno,¿qué te parece?

### "¿" + "verdad" (10 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Así Ezequiel puede conseguir más crack, ¿ verdad, tío?
  - Bromeas, ¿ verdad, papá?
  - Claro que sí, ¿ verdad?

### "…" + "-" (10 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bueno… -Sí.
  - Deja tu mensaje… -Directo al buzón.
  - Necesito hablar con él… -Hace frío.

### "-" + "¡" (9 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - B establece que si el oficial líder- ¡No son oficiales líderes!
  - Decía - ¡Basta ya!
  - EDIFICIO DEL SERVICIO DE IMPUESTOS INTERNOS - ¡Rápido, rápido!

### "-" + "¿" (9 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - ARCHIVOS TYRELL - ¿Es artificial?
  - Algo a lo que usted llama daño deliberado a la propiedad - ¿y qué?
  - Dice - ¿Quién se cree que es ese Rubliov?

### "," + "'" (8 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ella siempre dijo, 'Cásate, cásate con alguien grande.'
  - Le dije, 'Has buscado abajo del respaldo del sofá?'
  - Le he dado el nombre de Génesis, 'Nacimiento'.

### "--" + "¿" (8 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cinco-- ¿listo?
  - Mi tesis es sobre-- ¿Conoces al pintor Egon Schiele?
  - No sé qué-- ¿Qué estás buscando?

### "'" + "y" (7 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Di 'buenas noches' y vete a tu casa.
  - Di 'por favor' y lo haré.
  - Esa es la de 'impulso' y esa también.

### "," + "no" (6 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,no lo estaba.
  - No sé,no es exactamente el tipo actor.
  - No,no es eso.

### "¿" + "no" (6 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Es la vajilla del camafeo, ¿ no es cierto?
  - Esos relieves están pintados, ¿ no?
  - Hermoso, ¿ no?

### "," + "gracias" (5 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,gracias por todo lo que has hecho.
  - Bueno,gracias.
  - No,gracias.

### "," + "me" (5 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,me encantaría saber más sobre ella.
  - Fue exactamente lo que el Padre Jean,me contestó.
  - Justo lo que le interesa,me parece.

### "--" + "¡" (5 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cuando oigan-- ¡Eso es!
  - Dentro del auto hay-- ¡Ve despacio!
  - Es gracias a él-- ¡Maldición!

### "Lava" + "te" (5 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Lavate la boca antes de hablar de mí.
  - Lavate la cara antes de ir a la escuela.
  - Lavate la cara y las manos.

### "hacer" + "te" (5 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No, voy a hacerte algo especial, si no re importa.
  - Sí, tienes que hacerte camino por allí.
  - Tengo que hacerte un par de preguntas.

### "¿" + "qué" (5 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Además, ¿ qué tenía yo que ver con eso?
  - Cleopatra, por ejemplo, ¿ qué sabe de ella?
  - Disculpa, pero ¿ qué quieres decir con esto?

### "," + "-" (4 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Entonces vengan y juzgaremos, - Dijo Dios.
  - Mira aquí, campeón, -Tengo relojes de oro, anillos de diamante.
  - Oh, - ¡Eso fue todo!

### "/" + "TH" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hola, MU/TH/UR.
  - Lo lamento, MU/TH/UR, el Yautja ha muerto.
  - Tessa, mensaje entrante de MU/TH/UR.

### "/" + "UR" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hola, MU/TH/UR.
  - Lo lamento, MU/TH/UR, el Yautja ha muerto.
  - Tessa, mensaje entrante de MU/TH/UR.

### "MU" + "/" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hola, MU/TH/UR.
  - Lo lamento, MU/TH/UR, el Yautja ha muerto.
  - Tessa, mensaje entrante de MU/TH/UR.

### "Rompi" + "ste" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Rompiste la regla cardinal de nuestra relación.
  - Rompiste la regla.
  - Rompiste las reglas.

### "TH" + "/" (4 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hola, MU/TH/UR.
  - Lo lamento, MU/TH/UR, el Yautja ha muerto.
  - Tessa, mensaje entrante de MU/TH/UR.

### "'" + "es" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Creo que 'él' es más bien 'ella'.
  - El 'soldado' es un cargo de más responsabilidad.
  - Más importante que 'qué' es 'cuándo' .

### "," + "Hu" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hu,Hu,Hu,Hu, ¡date prisa, idiota!
  - Hu,Hu,Hu,Hu, ¡date prisa, idiota!
  - Hu,Hu,Hu,Hu, ¡date prisa, idiota!

### "," + "Jack" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mira,Jack, no contraté a esa muchacha por su grito.
  - Oye,Jack, ¿adónde carajo vas?
  - Oye,Jack, ¿de qué estás hablando?

### "," + "de" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - He estado pensando mucho en tomarme un descanso,de hecho.
  - Já disse, eu não gosto,de patê!
  - Não, obrigado Eu não gosto,de patê.

### "," + "el" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No son tantas líneas, pero es una parte clave,el hijo de Rachel.
  - No,el punto es revisarlo juntos y decidir.
  - Sí,el papel que no quisiste.

### "," + "esa" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,esa es la pregunta que plantea la película,¿no?
  - Mierda,esa es Rachel Kemp.
  - Sabes,esa era la oficina de la abuela antes de que se enfermara.

### "," + "eso" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,eso espero.
  - Sí,eso ayuda un poco.
  - Sí,eso dices tú.

### "," + "sí" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,sí nos fuimos después de esto.
  - Eh,sí.
  - Oh,sí.

### "--" + "No" (3 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Es que-- No lo sé.
  - Molesto -- No.
  - Oye, es bueno verte-- No, estaba mirando una película fantástica en la TV.

### "en" + "The Running Man" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Anteriormente enThe Running Man.
  - Aquí enThe Running Man creemos en seguir las reglas.
  - Hoy es tu última oportunidad del año estar enThe Running Man!

### "hola" + "'" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Con el 'hola' ya era tuya.
  - Luego se paró el motor y alguien dijo 'hola'.
  - Oye un tiro, y antes de eso, 'hola'.

### "mantener" + "te" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Deberías mantenerte en contacto con el señor Smith.
  - Tendrás que mantenerte entero muchos años.
  - Tienes que mantenerte lejos del ático.

### "qué" + "'" (3 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La pregunta correcta no es 'qué', sino 'a quién'.
  - Más importante que 'qué' es 'cuándo' .
  - Pues no saben dónde van, ni por qué'.

### "'" + "." (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Di: 'Ah' .
  - Más importante que 'qué' es 'cuándo' .

### "'" + "se" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Aunque eso de 'Dios mío' se lo hayas añadido.
  - Por si 'mirar' se vuelve 'comprar'.

### "," + "Sissel" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hay dos versiones de cómo Gustav conoció a su esposa,Sissel.
  - Seis meses después,Sissel estaba embarazada.

### "," + "está" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,está bien.
  - Oh,está fuera de lugar.

### "," + "papá" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hola,papá.
  - No vamos a trabajar juntos,papá.

### "," + "pero" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No voy a dar un discurso,pero siento esta gratitud brotando en mí.
  - Sé que piensas que eres incapaz de cuidar,pero tú estabas ahí para mí.

### "," + "por" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Abre la puerta,por favor.
  - Me gustaría ese jarrón,por ejemplo.

### "," + "sólo" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Eh,sólo un café.
  - No quiero entrometerme,sólo siento que podría ser relevante.

### "," + "¡" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero mi señor,¡Ese no es un oso!
  - Rach,¡tengo tu teléfono!

### "--" + "por" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Es sólo una muestra de agradecimiento-- por todo.
  - News Today-- por una suma que no se reveló.

### "Cinco Familias" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Con los jefes de las 'Cinco Familias'.
  - Una de las 'Cinco Familias', quizá todas.

### "Desert" + "‑" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Muro climático de Desert‑Tundratown puerta de acceso, ¡rápido!
  - Muro climático de Desert‑Tundratown, puerta de acceso.

### "Family Feud" + "»" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ah, «Family Feud».
  - Estoy viendo «Family Feud».

### "Rómpan" + "lo" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Rómpanlo todo.
  - Rómpanlo.

### "Wa" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Wa - alaikum salaam.
  - Wa - salaam alaikum.

### "adiós" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Creo que lo que se dice es 'adiós'.
  - Guárdalo en tu corazón y adiós'.

### "aquí" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Al fin llegó al 'hasta aquí'.
  - Él dice 'la encontré' y luego repite 'está aquí'.

### "contar" + "me" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sabes que puedes contarme cosas si necesitas hacerlo.
  - También deberías contarme todo sobre ello.

### "del" + "Titanic" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Claro, todos conocen las historias delTitanic.
  - Ios secretos sumergidos en el casco delTitanic.

### "dice" + ":" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ella dice: 'Sí, pero ésta se come mis palomitas'.
  - Él ve nuestra sombra y dice: 'Hágase la luz'.

### "dices" + ":" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Siempre dices: 'haz lo mejor'.
  - Una mañana te despiertas y dices: 'Mundo, te conozco.

### "enseñar" + "te" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Déjame enseñarte cómo se hace.
  - Déjame enseñarte.

### "gracias" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Creo que la palabra que buscas es 'gracias'.
  - Es mi manera de decir 'gracias'.

### "mitad" + "-" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Soy mitad - espagueti, soy mitad - negro, No le tengo miedo a nadie.
  - Soy mitad - espagueti, soy mitad - negro, No le tengo miedo a nadie.

### "prometo" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dije 'te lo prometo'.
  - Dijiste: 'Dime y luego te lo prometo'.

### "si" + "'" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ese es un gran 'si'.
  - Yo no necesito oír ningún 'si'.

### "un" + "judío" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tú pareces a unjudío.
  - Yo sé que hay unjudío aquí.

### "vez" + "un" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Había una vezunaprincesa que vivía en un palacio muy grande.
  - Has visto alguna vezun muerto?

### "¿" + "cómo" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Chicos, ¿ cómo los hace sentir esto?
  - Señor Long, mi buen amigo, ¿ cómo está usted?

### "¿" + "eh" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Aún crees que soy tu criado, ¿ eh?
  - No me recuerdas, ¿ eh?

### "¿" + "vale" (2 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Olvidémoslo, ¿ vale?
  - Una vez más, ¿ vale?

### "‑" + "Tundratown" (2 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Muro climático de Desert‑Tundratown puerta de acceso, ¡rápido!
  - Muro climático de Desert‑Tundratown, puerta de acceso.

### "'" + "a" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pero sólo le llamaba su 'dama' a una.

### "'" + "de" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Muy 'hielo' de tu parte haber venido.

### "'" + "durante" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ahora sólo usa la palabra 'maricón' durante el sexo.

### "'" + "eligieron" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Porque muchos otros 'payasos' eligieron este lugar.

### "'" + "en" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Necesito una 'buena bola' en diez o quince minutos.

### "'" + "está" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La palabra 'Navidad' está mal escrita.

### "'" + "lo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Fue lo de 'virgen' lo que llamó la atención.

### "'" + "los" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Los de 'Tráfico' los llaman 'Casa de Huéspedes'.

### "'" + "mis" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Puedo 'lamer' mis propias bolas, muchas gracias.

### "'" + "más" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Un 'bien' más y no le voy a creer.

### "'" + "o" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cómprate un 'filete de béisbol' o algo.

### "'" + "otra" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vaya, 'La Voz de su Amo' otra vez.

### "'" + "que" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Todo el mundo acepta la 'verdad' que le decimos.

### "'" + "ya" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Con el 'hola' ya era tuya.

### "," + "!" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - A tu lado, !

### "," + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vendi la casa, pague las cfacturas, , me puse firme con mis deudas.

### "," + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Es unazona erógena muy sensible, .

### "," + "Hermano" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,Hermano Ministro -No?

### "," + "Jean" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ahora nosotros rezaremos por el Padre,Jean y los muchachos.

### "," + "Karin" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Después de la guerra,Karin se casó y se quedó con la casa familiar.

### "," + "L" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - U,L,l,O.

### "," + "Michael" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Estoy demasiado cansado,Michael.

### "," + "Nora" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No digas eso,Nora.

### "," + "O" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - U,L,l,O.

### "," + "Running" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La tensión es mayor que nunca,Running fans.

### "," + "Usted" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mi Dios,Usted tiene el caso del Destripador.

### "," + "a" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,a veces me ayuda con investigaciones.

### "," + "anda" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,anda al teléfono público.

### "," + "aquí" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,aquí no.

### "," + "arriba" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La página anterior,arriba.

### "," + "así" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vale,así que tardó todos esos años en llegar aquí.

### "," + "claro" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí,claro.

### "," + "cristãos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Amentira é muito poderosa os,cristãos se digladiam.

### "," + "cuando" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quiero decir,cuando sales al escenario, es tan contraintuitivo.

### "," + "desapareció" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - El ruido que hacían sus padres,desapareció.

### "," + "durante" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La fabricación de papel fue,durante mucho tiempo, un secreto de Estado.

### "," + "dímelo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Gracias por hacer el trabajo que haces, y si hay algo,dímelo.

### "," + "e" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Porque você se chama Kippelstein,e não Bonnet.

### "," + "ejercicio" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Agarren sus cuadernos para el,ejercicio de Álgebra.

### "," + "ella" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Y recuerda,ella nunca le ha contado esto a nadie.

### "," + "en" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,en realidad no.

### "," + "entendemos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí,entendemos que ustedes eran amigas de ambas damas.

### "," + "era" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero lo que a la casa le disgustaba más que el ruido,era el silencio.

### "," + "es" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No hace mal cuando,es para bien.

### "," + "espera" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,espera.

### "," + "esto" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,esto no trata sobre mi madre.

### "," + "estoy" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí,estoy bien.

### "," + "filmo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sabes,filmo con amigos.

### "," + "fue" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Y sí,fue bonito, papá.

### "," + "hablamos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,hablamos de vez en cuando.

### "," + "hablando" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Así que ella está aquí,hablando con su hijo.

### "," + "importantes" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hay más cosas,importantes.

### "," + "ira" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Esta frustración,ira, sensación de injusticia, la carga de la responsabilidad.

### "," + "jefe" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Es el personaje de una tira cómica,jefe.

### "," + "jovencito" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero temo,jovencito, que su posición no es negociable.

### "," + "l" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - U,L,l,O.

### "," + "le" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí,le diste.

### "," + "lhes" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - E como lhes foi dado muito ,lhes será exigido muito.

### "," + "logré" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí, me ataron en un árbol, pero,logré huir.

### "," + "maldita" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero vas a hacer esta película,maldita sea.

### "," + "mierda" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Oh,mierda.

### "," + "ni" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Perdón,ni siquiera puedo hablar bien.

### "," + "nos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Aqueles que deviam nos guiar,nos traem.

### "," + "perguntas" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Por que você sempre me faz,perguntas idiotas?

### "," + "primero" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,primero come otra cosa.

### "," + "probablemente" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hace veinte años,probablemente, desde que la vi.

### "," + "prósperas" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vocês todos vêm de famílias,prósperas.

### "," + "puedes" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,puedes acercarte más.

### "," + "solía" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Cuando era pequeña,solía escuchar sus conversaciones a través de esta estufa.

### "," + "supongo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí,supongo.

### "," + "sé" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Y Jakob,sé que las cosas son difíciles en el frente doméstico.

### "," + "te" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,te quiero para el papel principal.

### "," + "tengo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí,tengo que hacer esto.

### "," + "tiene" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,tiene este proyecto.

### "," + "todo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu cuerpo,todo tu cuerpo grita al salir a enfrentarte al público.

### "," + "todos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Todos los Gillet son de Lyon Y,todos fabrican seda.

### "," + "tomemos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vamos,tomemos algo.

### "," + "tus" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tienea la felicidad de encontrar,tus amigos.

### "," + "tuvo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No,tuvo que irse.

### "," + "un" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,un poco sobreescrito.

### "," + "una" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bueno,una última pregunta antes de dejarte ir.

### "," + "vamos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Está bien,vamos hombres, ya despejen el pasaje.

### "," + "y" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Se abrazan,y él se va.

### "," + "ya" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No quiero decepcionarlo,ya sabes.

### "," + "yo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quiero decir,yo también te quiero.

### "," + "«" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ah, «Family Feud».

### "-" + "Abdias" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - En el certificado de nacimiento - Abdias.

### "-" + "Apagado" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Motor de tercera etapa - Apagado.

### "-" + "Aventuras" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ficción - Aventuras.

### "-" + "Buena" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - JUEGO UNO - Buena suerte, caballeros.

### "-" + "Dijo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Entonces vengan y juzgaremos, - Dijo Dios.

### "-" + "Este" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Déjame decirte - Este es mi último vuelo antes de retirarme.

### "-" + "Gracias" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vamos, vuelvo en seguida - Gracias.

### "-" + "Heathrow" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bienvenidos a Londres- Heathrow.

### "-" + "I" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - F- I-E-L-D.

### "-" + "Laraña" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Parece que Ella- Laraña se ha estado divirtiendo.

### "-" + "Lo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No, tú- Lo siento, tienes razón.

### "-" + "Nada" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mi punto fuerte es mi filosofía - Nada arriesgado, nada ganado.

### "-" + "Normal" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rotación de programa - Normal.

### "-" + "Todo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No tengas-- - Todo está bien.

### "-" + "Vayamos" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - GASOLINERA - Vayamos al siguiente pueblo.

### "-" + "Whoo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rick Dalton - Whoo!

### "-" + "alaikum" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Wa - alaikum salaam.

### "-" + "cristiana" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Usted predica ciencia anti-blancos, anti- cristiana.

### "-" + "espagueti" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Soy mitad - espagueti, soy mitad - negro, No le tengo miedo a nadie.

### "-" + "esta" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La realidad tiene un problema - esta siempre es verdadera.

### "-" + "estaba" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo abrí la caja - estaba vacía.

### "-" + "fueras" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - RE-CONECTANDO - fueras tú.

### "-" + "gallinas" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ordeñadores de chivas, tienta - gallinas, cazadores de pulgas!

### "-" + "he" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Beber o no beber - he ahí el dilema.

### "-" + "igual" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ella era EVP -estado vegetativo persistente- igual que Lydia.

### "-" + "me" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo - me dijo -, temía que tu papito no te dejara casar conmigo.

### "-" + "mientras" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Y,,, qué se siente bajo el agua - mientras arriba el enemigo está al acecho?

### "-" + "negativo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - B- negativo.

### "-" + "negro" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Soy mitad - espagueti, soy mitad - negro, No le tengo miedo a nadie.

### "-" + "ni" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Especial - ni hablar.

### "-" + "no" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Se han vuelto locos - no les importa nada más que el asesino.

### "-" + "pescar" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tengo muchas aficiones - pescar y escalar, por ejemplo.

### "-" + "primavera" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hay cuatro estaciones en un año - primavera, verano, otoño, e invierno.

### "-" + "respuesta" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sándwich con mantequilla - respuesta poco entusiasta.

### "-" + "salaam" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Wa - salaam alaikum.

### "-" + "se" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ser calvo tiene al menos una ventaja - se ahorra mucho en champú.

### "-" + "suaves" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Las mujeres son como un pétalo de una rosa - suaves y hermosas.

### "-" + "tapas" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Habrá cambios - tapas para los tornillos en la plataforma.

### "-" + "tenemos" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Y hoy simplemente fue perfecto - tenemos información suficiente.

### "-" + "todos" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Atraer hacia sí -de los transeúntes- todos los demonios de la Tierra.

### "-" + "un" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - El cielo está sombrío y gris - un cielo típico de la estación de lluvias.

### "-" + "va" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Viernes negro para el asado - va en picado.

### "-" + "we" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - We get inside, turn on that switch, light the clock tower - we find her house.

### "--" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No tengas-- - Todo está bien.

### "--" + "Bueno" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bueno pero quédate se-- Bueno, por eso, quédate sentado acá.

### "--" + "Dale" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sólo-- Dale las gracias a tu hermano.

### "--" + "Era" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Era-- Era yo.

### "--" + "Es" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sí, no quise-- Es así.

### "--" + "Escucha" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tu bebé va a estar-- Escucha.

### "--" + "Estoy" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Est-- Estoy muy mejor, papá.

### "--" + "Frenesí" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Y eso nos lleva al presente-- Frenesí Para Ambos Sexos.

### "--" + "Gracias" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - De hecho, me puede dar-- Gracias.

### "--" + "Justo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sala de enlace-- Justo allí está la sala de enlace.

### "--" + "Las" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Desde que-- Las cosas están mejor.

### "--" + "Oye" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sólo-- Oye, ve más despacio.

### "--" + "Te" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No te he visto-- Te ves espléndida.

### "--" + "Tú" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tú-- Tú estás precisando anteojos.

### "--" + "Un" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tuve un sueño y-- Un momento.

### "--" + "Vamos" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vamos a-- Vamos a la azotea.

### "--" + "Vete" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sé-- Vete.

### "--" + "Yo" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Nosotros-- Yo estaba en el bosque, tú en el auto.

### "--" + "a" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Quisiera ir de visita-- a Nueva Inglaterra.

### "--" + "dos" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cielos-- dos años.

### "--" + "en" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo-yo-yo te estaba mirando en-- en Free-Vee!

### "--" + "eso" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ah, sí, eso fue-- eso fue hace mucho tiempo.

### "--" + "tú" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hagamos una aventura juntos-- tú y yo finalmente.

### ".." + "ver" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí..ver que pasara con el cargo de locutor.

### "/" + "cocinada" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La carne no está lo bastante hecha/cocinada/pasada.

### "/" + "pasada" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La carne no está lo bastante hecha/cocinada/pasada.

### "/" + "pelo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Es húmedo y redondo, rodeado de pestañas/pelo.

### ":" + "no" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Repito:no entre al espacio aéreo del aeropuerto.

### "AMOR" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - PRIMER AMOR - ¿No ves lo maravilloso que será?

### "Ah" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Di: 'Ah' .

### "Artículos Valiosos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Uno decía 'Vestuario' y el otro 'Artículos Valiosos'.

### "Azul Verdadero" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - De eso se trata 'Azul Verdadero'.

### "Baja" + "la" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bajala voz.

### "Barras y Estrellas" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Son de 'Barras y Estrellas'.

### "Blanco" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Basta de esta mierda de 'Sr. Blanco'.

### "Boleto" + "s" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Boletos, por favor.

### "Boston" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - En octubre hará tres años que me mudé a Boston .

### "CAIRO" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - EL CAIRO - ¡Sí, entrenador, sí!

### "CATÓLICO" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - MUNDO CATÓLICO - ¿QUÉ ES LA VIDA SIN ALGO DE CULPA?

### "CONECTANDO" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - RE-CONECTANDO - fueras tú.

### "Casa de Huéspedes" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Los de 'Tráfico' los llaman 'Casa de Huéspedes'.

### "Come" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Come -Gracias.

### "Decía" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Decía - ¡Basta ya!

### "Di" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Di: 'Ah' .

### "Dice" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dice - ¿Quién se cree que es ese Rubliov?

### "Dicen" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dicen: 'No tienes que hacer esto'.

### "Dije" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dije: 'Un día seré un policía del espacio verde'.

### "Dijiste" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dijiste: 'Dime y luego te lo prometo'.

### "Dios" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Solo hace referencia a 'la mano izquierda de Dios'.

### "EVP" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ella era EVP -estado vegetativo persistente- igual que Lydia.

### "Es" + "¡" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Es¡El hombre que corre!

### "Especial" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Especial - ni hablar.

### "Est" + "¡" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Est¡Estuve a punto!

### "Ficción" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ficción - Aventuras.

### "Fuerzas de la Naturaleza" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Parece que no has visto 'Fuerzas de la Naturaleza'.

### "GASOLINERA" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - GASOLINERA - Vayamos al siguiente pueblo.

### "Gorro" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Su nombre es Gorro -Para con eso.

### "Hércules" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Es un avión grande, entonces lo llamé 'el Hércules'.

### "INTERNOS" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - EDIFICIO DEL SERVICIO DE IMPUESTOS INTERNOS - ¡Rápido, rápido!

### "Impote" + "nte" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Impotente de mierda.

### "Interrumpimos" + "Los" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - InterrumpimosLos Americanos para una actualización en vivo deThe Running Man.

### "Itabaiana" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Itabaiana -hasta aquí a pié por la ruta.

### "Joseph" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mira el pequeño Joseph -¿Tienes jalea?

### "Kitchen" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Como Store Kitchen .

### "La Voz de su Amo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vaya, 'La Voz de su Amo' otra vez.

### "Llevé" + "moste" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Llevémoste al vestuario.

### "Maddalena" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Maddalena -Sigo todavía aquí.

### "Ministro" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No,Hermano Ministro -No?

### "Molesto" + "--" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Molesto -- No.

### "Moveos" + "¡" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Moveos¡!

### "Nacimiento" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le he dado el nombre de Génesis, 'Nacimiento'.

### "Navidad" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La palabra 'Navidad' está mal escrita.

### "No" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No -¿Quieres algo especial?

### "Paga" + "rá" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pagará por esto.

### "Papa" + "!" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ya estoy libre, Papa !

### "Papa" + "?" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Papa ?

### "Pequeño Chino" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Un tipo al que llaman 'Pequeño Chino'.

### "Pero" + "¿" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero¿qué sé yo?

### "Pisam" + "os" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pisamos la misma tierra.

### "Quinta" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pero todo esto cambia con la Quinta .

### "RACHEL CHU" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - RACHEL CHU - ¿HAY UN ANILLO?

### "RECHAZADO" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - RECHAZADO - ¡Evelyn, espera!

### "RECLUSA" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - RECLUSA - ¿DÓNDE ESTÁ EL MALDITO CEMENTERIO?

### "Repito" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Repito:no entre al espacio aéreo del aeropuerto.

### "Rick Dalton" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rick Dalton - Whoo!

### "Rompi" + "mos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Rompimos.

### "Set" + "enta y cinco dividido por cinco son quince." (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Setenta y cinco dividido por cinco son quince.

### "Señor de los Anillos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tiene una mochila del 'Señor de los Anillos'.

### "Shinachiku" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tallarines Shinachiku .

### "Sol" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La 'Tercera Roca desde el Sol', ahora lo entiendo.

### "Sonrisas" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Oye, 'Sonrisas'.

### "Sr." + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero no pienses que te voy a llamar 'Sr.'

### "Sí" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sí -Sí.

### "Sí" + "­" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí­, me habló de ti.

### "TYRELL" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - ARCHIVOS TYRELL - ¿Es artificial?

### "Tadashi Doi" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tadashi Doi , en condición crítica.

### "Tirador" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vuelve a repartir, 'Tirador'.

### "Toca" + "n" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tocan música argelina todo el tiempo.

### "Tráfico" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Los de 'Tráfico' los llaman 'Casa de Huéspedes'.

### "UNO" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - JUEGO UNO - Buena suerte, caballeros.

### "Vamos" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vamos .

### "Ven" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ven .

### "Vestuario" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Uno decía 'Vestuario' y el otro 'Artículos Valiosos'.

### "Y" + "justo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Yjusto aquí, en la periferia de las esfinges, una gran cocina.

### "Y" + "por" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ypor qué corta?

### "Y" + "tus" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ytus padres?

### "Y" + "¿" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Y¿Usted, Mi señor?

### "Yo" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo - me dijo -, temía que tu papito no te dejara casar conmigo.

### "abismo" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Llámalo un abismo - ¡Qué profundo que es!

### "abuela" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sólo escucharé a mi abuela .

### "accidente" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Eso debe haber sido antes de su 'accidente'.

### "actores" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Somos actores , le traemos felicidad a la gente.

### "adoptar" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Se decidió a adoptar .

### "aficiones" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tengo muchas aficiones - pescar y escalar, por ejemplo.

### "agua" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Y,,, qué se siente bajo el agua - mientras arriba el enemigo está al acecho?

### "alguna" + "idea" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tienes algunaidea?

### "americano" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Lo que dije fue: 'Dios existe y es americano'.

### "amo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hay quien dice 'te amo' y no significa nada.

### "amor" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La palabra que usé fue 'amor'.

### "anoche" + "'?" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Gracias por la fiesta de anoche'?

### "aquí" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sólo estoy diciendo , podríamos construir una vida juntos aquí .

### "arriba" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mano derecha sobre la escoba y digan: 'arriba'.

### "asado" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Viernes negro para el asado - va en picado.

### "así" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Siempre es así: nadie vio nada, nadie sabe nada.

### "así" + "­" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Yo soy así­.

### "ataud" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hitler puede tener el mundo, pero no puede llevarselo al ataud .

### "atención" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Trastorno por déficit de la atención .

### "año" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hay cuatro estaciones en un año - primavera, verano, otoño, e invierno.

### "beber" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Beber o no beber - he ahí el dilema.

### "besar" + "lo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Quiero besarlo.

### "bestia" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Hubo un pez que era 'la bestia'.

### "bien" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Un 'bien' más y no le voy a creer.

### "bien" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mis pies nunca se han visto tan bien .

### "blancos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero tienen un gran letrero que dice 'Solo blancos'.

### "bola" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Necesito una 'buena bola' en diez o quince minutos.

### "busca" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Estar en un cartel de 'se busca'.

### "béisbol" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Cómprate un 'filete de béisbol' o algo.

### "caballero" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Buenos días, caballero -Ven, lobezno bonito.

### "cafetera" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Este coche es una cafetera .

### "café" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sí, cantamos 'Eres la crema en mi café'.

### "caja" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo abrí la caja - estaba vacía.

### "callado" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sea más cuidadoso y quédese callado -Lo sigo.

### "cambios" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Habrá cambios - tapas para los tornillos en la plataforma.

### "caridad" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Más la mayor de ellas es la caridad'.

### "cita" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Lo nombre 'El juego de la cita'.

### "cocinada" + "/" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La carne no está lo bastante hecha/cocinada/pasada.

### "cohete" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Déjame ver tu 'cohete'.

### "comer" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - En tres meses estarán buenos para comer -Para el día de los padres.

### "comprar" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Por si 'mirar' se vuelve 'comprar'.

### "consumiste" + "?" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cuantas tazas de café consumiste ?

### "contento" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Como dicen: 'En boca cerrada, corazón contento'.

### "corazón" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Lo llevo en mi corazón'.

### "cosas" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No arreglo ese tipo de cosas'.

### "crudo" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dile a tu madre que lo quieres crudo -¿Por qué?

### "cuatro" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Creí haber oído 'los cuatro'.

### "cuándo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Más importante que 'qué' es 'cuándo' .

### "dama" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero sólo le llamaba su 'dama' a una.

### "darle" + "una" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Que he venido a darleuna vuelta a mi tumba.

### "de" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La extracción del código puede resultar en de -resolución del transportista.

### "de" + "The" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - InterrumpimosLos Americanos para una actualización en vivo deThe Running Man.

### "de" + "The Running Man" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La bebida oficial deThe Running Man.

### "decirte" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Déjame decirte - Este es mi último vuelo antes de retirarme.

### "demás" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Se aplica a ti y a todos los demás -en este cuarto.

### "descomunal" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - El robo de las riquezas peruanas perpetrado por los españoles fue descomunal .

### "desempleada" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ella está desempleada -No existe más eso.

### "dicen" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Como dicen: 'En boca cerrada, corazón contento'.

### "diciendo" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sólo estoy diciendo , podríamos construir una vida juntos aquí .

### "digan" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mano derecha sobre la escoba y digan: 'arriba'.

### "digo" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le digo: 'quieto' y el dragón se queda quieto.

### "dijimos" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le dijimos: 'Así es la vida'.

### "dijo" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo - me dijo -, temía que tu papito no te dejara casar conmigo.

### "dos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Un poco del viejo 'uno-dos'.

### "ducha" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ve a la segunda ducha -¿Porque no la bañera?

### "el" + "viejo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ni siquiera sabemos de dónde sacó elviejo la cinta.

### "el" + "visor" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No vas amirar por elvisor?

### "ella" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Creo que 'él' es más bien 'ella'.

### "ella" + "?" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Cuantos vasos de jugo se tomó ella ?

### "ellos" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Moreau es uno de ellos -¿ Verdad?

### "enamorados" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero para una clienta especial como tú tengo 'enamorados'.

### "encontré" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Él dice 'la encontré' y luego repite 'está aquí'.

### "enfermería" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No, está en la enfermería -¿Ah, sí?

### "escena" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pensé que era uno de esos 'artistas de escena'.

### "eso" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Es sólo eso: una historia.

### "esto" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dicen: 'No tienes que hacer esto'.

### "etapa" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Motor de tercera etapa - Apagado.

### "evitarlo" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Si no puedes evitarlo , métete en ello.

### "ex" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - El es un ex -terrorista checheno.

### "experiencia" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Puedo decirles por experiencia -que eso requiere un verdadero campeón.

### "favor" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Di 'por favor' y lo haré.

### "filosofía" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mi punto fuerte es mi filosofía - Nada arriesgado, nada ganado.

### "floresta" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Él estaba conmigo en la floresta -Ah, era tú.

### "frita" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La señora Cook está siendo «frita» por el presidente, que quiere despedirla.

### "fue" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Lo que dije fue: 'Dios existe y es americano'.

### "fumar" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - François, yo te prohíbo fumar -Es paja de choclo.

### "gente" + "?" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ha habido un hombre blanco en la historia que le haya ayudado a su gente ?

### "gracias" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Oye, muchísimas gracias -por tu tiempo.

### "gris" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - El cielo está sombrío y gris - un cielo típico de la estación de lluvias.

### "haber" + "me" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Gracias por haberme invitado a cenar.

### "habrá" + "próxima" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No habrápróxima vez.

### "hacer" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Una sola persona decide qué voy a hacer: yo.

### "hecha" + "/" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La carne no está lo bastante hecha/cocinada/pasada.

### "hielo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Muy 'hielo' de tu parte haber venido.

### "hija" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Su hija .

### "hipotéticos" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Los lectores de ciencia ficción son adeptos a escenarios hipotéticos .

### "hizo" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Adrienne lo hizo -¿Ella es tu hermana?

### "homosexuales" + ".." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Soy un gran modelo para los homosexuales ..

### "hotel" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - El pequeño restaurante de hoy mañana puede ser un gran hotel .

### "hoyo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dios es fanático del 'tiro al hoyo'.

### "imbécil" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pétain es un imbécil -No concuerdo.

### "impulso" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Esa es la de 'impulso' y esa también.

### "intenciones" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Estabas diciendo algo de 'la mejor de las intenciones'.

### "intención" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Estabas diciendo algo sobre 'la mejor intención'.

### "juego" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Su cabeza está en juego - ¡Si es que no se ha dado cuenta!

### "juicio" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tu dijiste 'El día del juicio'.

### "la" + "hora" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Va a ser lahora de comer.

### "lado" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Es la 'puerta a ningún lado'.

### "lamer" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Puedo 'lamer' mis propias bolas, muchas gracias.

### "las" + "joyas" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Mira lasjoyas de plástico.

### "latín" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo estudiaba latín -¿Dónde?

### "leer" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Lo acabo de leer - ¿Sí?

### "leer" + "te" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Tienes que leerte este libro.

### "llama" + "Jorge Castro" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Se llamaJorge Castro, me está apuntando con unapistola.

### "llamada" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Dieciocho años sin decir ni mu, ni una carta, ni una llamada .

### "locos" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Se han vuelto locos - no les importa nada más que el asesino.

### "lorry" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - A un camión se lo llama «lorry» en Reino Unido.

### "los" + "judíos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Me encantan losjudíos.

### "lugar" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Hay muchos aquí que prefieren están en otro lugar .

### "luz" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Él ve nuestra sombra y dice: 'Hágase la luz'.

### "mamá" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Mira mamá , esta es Manuela.

### "mantequilla" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sándwich con mantequilla - respuesta poco entusiasta.

### "maricón" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ahora sólo usa la palabra 'maricón' durante el sexo.

### "mejor" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Siempre dices: 'haz lo mejor'.

### "mensch" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Se maldito mensch , chico.

### "mirar" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Por si 'mirar' se vuelve 'comprar'.

### "mitad" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sus probabilidades de sobrevivir son de mitad y mitad'.

### "mitad" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Así que de esta forma vive la otra mitad .

### "montaje" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Los construyó sobre una línea de montaje .

### "muerta" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pensé que habías muerta , hija puta.

### "muito" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - E como lhes foi dado muito ,lhes será exigido muito.

### "mundo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Con todas las riquezas del mundo'.

### "mágica" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La llamamos la teoría de 'la bala mágica'.

### "mío" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Aunque eso de 'Dios mío' se lo hayas añadido.

### "nacimiento" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - En el certificado de nacimiento - Abdias.

### "nativo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - El examen es sobre 'El hijo nativo'.

### "necesito" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bas dice que sólo lo necesito -sin distracciones.

### "no" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - A otra persona, pero a mí no'.

### "no" + "‑" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Yo solo no‑ no entiendo.

### "noches" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Di 'buenas noches' y vete a tu casa.

### "noches" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Y se fue sin siquiera decir «buenas noches».

### "nosotros" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Amigo, esta vez no es 'nosotros'.

### "nuestra" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - O, si prefieres, puedes decir 'nuestra'.

### "oídos" + "?" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Así que te lamen los oídos ?

### "pa" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - A caballo vamos pa'l monte.

### "palomitas" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ella dice: 'Sí, pero ésta se come mis palomitas'.

### "payasos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Porque muchos otros 'payasos' eligieron este lugar.

### "pensando" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Y está pensando: 'Te ha ido bien.

### "perdidas" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Balas perdidas -Eso te hace convertirte en hombre.

### "perfecto" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Y hoy simplemente fue perfecto - tenemos información suficiente.

### "pero" + "¿" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Perdón por aparecer así, pero¿por qué no respondes mis llamadas?

### "pesado" + "​." (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Déjame ayudarte con ese paquete pesado​.

### "pestañas" + "/" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Es húmedo y redondo, rodeado de pestañas/pelo.

### "poder" + "te" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Espero poderte ver mañana.

### "problema" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La realidad tiene un problema - esta siempre es verdadera.

### "procrastinar" + "–" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Deja de procrastinar–a partir de mañana.

### "programa" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rotación de programa - Normal.

### "propiedad" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Algo a lo que usted llama daño deliberado a la propiedad - ¿y qué?

### "pud" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No sé nada, todavía hace falta medio pud .

### "que" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - No quiero que -reescribas.

### "quejaré" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Me quejaré -¿Para quién?

### "quiera" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Esto significa 'puedo hacer lo que quiera'.

### "quieto" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le digo: 'quieto' y el dragón se queda quieto.

### "quién" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - La pregunta correcta no es 'qué', sino 'a quién'.

### "recuerdos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Como podrán haber notado, todas contienen 'recuerdos'.

### "rodeen" + "lo" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Todos, corran hasta ese árbol, rodeenlo y regresen.

### "rollo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Esto se llama 'cambio de rollo'.

### "rosa" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Las mujeres son como un pétalo de una rosa - suaves y hermosas.

### "sacerdotes" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Prohibido por los sacerdotes -¿Porque?

### "sale" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ud. es uno de ésos de 'entra y sale'.

### "salida" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Primero encuentren a su 'amigo de salida'.

### "sea" + "," (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - O sea , que estas sola.

### "secretas" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Secretas, porque eso de 'agentes secretas'.

### "seguida" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Vamos, vuelvo en seguida - Gracias.

### "sensación" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Y como que quizás sea la mejor sensación -en el mundo, ¿sabes?

### "ser" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sobre 'Sé todo lo que puedes ser'.

### "será" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Siempre lo ha sido, y siempre así será'.

### "será" + "mejor" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Sena, serámejor que te vayas.

### "señal" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - La relación señal -ruido es insoportable.

### "siempre" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Y Cenicienta y el príncipe vivieron felices para siempre'.

### "soldado" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - El 'soldado' es un cargo de más responsabilidad.

### "son" + "judíos" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Bonnet, Dupré y Lafarge sonjudíos.

### "suficiente" + "»" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Las personas codiciosas experimentan una mentalidad de «nunca es suficiente».

### "sé" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sí, lo sé -Bueno.

### "sí" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Atraer hacia sí -de los transeúntes- todos los demonios de la Tierra.

### "tabla" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ella solía ser una tabla - ¿Cuándo fue que se vino tan grande?

### "tarde" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Por favor, avise que llegaré tarde'.

### "tema" + "muy" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Teniendo en cuenta que es un temamuy delicado.

### "ti" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Sólo hay un derecho para ti - ¡la muerte!

### "tiempo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Pero ha dicho 'Casi todo el tiempo'.

### "tienta" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ordeñadores de chivas, tienta - gallinas, cazadores de pulgas!

### "todos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Que Dios os bendiga a todos'.

### "tower" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - We get inside, turn on that switch, light the clock tower - we find her house.

### "tres" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Tengo tres -Bueno.

### "turista" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Eres una 'turista'.

### "un" + "aprincesa" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Había una vezunaprincesa que vivía en un palacio muy grande.

### "una" + "idea" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Vale, sólo era unaidea.

### "unapelícula" + "podría" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Crees que unapelículapodría matar a unapersona?

### "va" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - También es conocida como la Sala 'Viene y va'.

### "veces" + ":" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - No digo mil veces: 'Acabo de comprar un sombrero.'

### "ventaja" + "-" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Ser calvo tiene al menos una ventaja - se ahorra mucho en champú.

### "verdad" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Todo el mundo acepta la 'verdad' que le decimos.

### "verde" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Dije: 'Un día seré un policía del espacio verde'.

### "vez" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - El muchacho montó un caballo por primera vez .

### "vida" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Le dijimos: 'Así es la vida'.

### "virgen" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Fue lo de 'virgen' lo que llamó la atención.

### "visitas" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Rosa, mira, visitas .

### "yo" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Se acabó el 'tú y yo'.

### "yo" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Me gusta hacer las cosas como las hago yo .

### "­" + "alaikum" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - As salaam ­alaikum!

### "¿" + "Giulia" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Por cierto, ¿ Giulia le mencionó mi idea?

### "¿" + "Verdad" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Moreau es uno de ellos -¿ Verdad?

### "¿" + "Y" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Bueno ¿ Y el fusil?

### "¿" + "adónde" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Anastasia, ¿ adónde vas?

### "¿" + "crecemos" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Claro, ¿ crecemos y tenemos hijos en este sumidero?

### "¿" + "desearíais" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Pero antes, ¿ desearíais hacerme alguna pregunta a mí?

### "¿" + "ha" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Disculpe, señora mía, ¿ ha perdido a su guía?

### "¿" + "ve" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Manganiello, ¿ ve a ese individuo?

### "¿" + "y" (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Las manos son bellas, ¿ y?

### "ánimos" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Eso le va a levantar los ánimos'.

### "área" + "." (1 occurrences)
- Predicted: None
- Actual: Space
- Examples:
  - Yo estuve en el área .

### "él" + "'" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Creo que 'él' es más bien 'ella'.

### "​​" + "fue" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Ninguno de los acusados ​​fue declarado culpable.

### "​​" + "ser" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Solía ​​ser una mujer amable e inteligente.

### "–" + "a" (1 occurrences)
- Predicted: Space
- Actual: None
- Examples:
  - Deja de procrastinar–a partir de mañana.

