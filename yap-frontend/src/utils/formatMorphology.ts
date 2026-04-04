import type { Morphology } from "../../../yap-frontend-rs/pkg";

export function formatMorphology(morphology: Morphology): string {
  const parts: string[] = [];

  if (morphology.person) {
    const personMap = {
      Zeroth: "0th-person",
      First: "1st-person",
      Second: "2nd-person",
      Third: "3rd-person",
      Fourth: "4th-person",
    } as const;
    parts.push(personMap[morphology.person]);
  }

  if (morphology.gender) {
    const genderMap = {
      Masculine: "masculine",
      Feminine: "feminine",
      Neuter: "neuter",
      Common: "common",
    } as const;
    parts.push(genderMap[morphology.gender]);
  }

  if (morphology.tense) {
    const tenseMap = {
      Past: "past tense",
      Present: "present tense",
      Future: "future tense",
      Imperfect: "imperfect tense",
      Pluperfect: "pluperfect tense",
    } as const;
    parts.push(tenseMap[morphology.tense]);
  }

  if (morphology.politeness) {
    const politeMap = {
      Informal: "informal",
      Formal: "formal",
      Elev: "elevated",
      Humb: "humble",
    } as const;
    parts.push(politeMap[morphology.politeness]);
  }

  if (morphology.case) {
    const caseMap = {
      Nominative: "nominative",
      Accusative: "accusative",
      Absolutive: "absolutive",
      Ergative: "ergative",
      Dative: "dative",
      Genitive: "genitive",
      Vocative: "vocative",
      Instrumental: "instrumental",
      Partitive: "partitive",
      Distributive: "distributive",
      Essive: "essive",
      Translative: "translative",
      Comitative: "comitative",
      Abessive: "abessive",
      Causative: "causative",
      Benefactive: "benefactive",
      Considerative: "considerative",
      Comparative: "comparative",
      Equative: "equative",
      Locative: "locative",
      Lative: "lative",
      Terminative: "terminative",
      Inessive: "inessive",
      Illative: "illative",
      Elative: "elative",
      Additive: "additive",
      Adessive: "adessive",
      Allative: "allative",
      Ablative: "ablative",
      Superessive: "superessive",
      Superlative: "superlative",
      Delative: "delative",
      Subessive: "subessive",
      Sublative: "sublative",
      Subelative: "subelative",
      Perlative: "perlative",
      Temporal: "temporal",
    } as const;
    parts.push(caseMap[morphology.case]);
  }

  return parts.length > 0 ? parts.join(", ") : "";
}
