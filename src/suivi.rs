//! Le suivi de FiveM pendant la chasse : chaque seconde, les programmes FiveM qui tournent décident de la suite.
//! FiveM se relance souvent seul en démarrant (mise à jour, changement de version) : tant que le jeu n'a pas tourné,
//! une disparition n'est pas une fin de chasse. mousquet le relance lui-même s'il ne revient pas.

pub const ATTENTE_FERMETURE: f64 = 10.0; // s après la fermeture du jeu avant de tout remettre
pub const ATTENTE_LANCEMENT: f64 = 90.0; // s : FiveM jamais apparu → relancé, puis tout est remis
pub const ATTENTE_DEMARRAGE: f64 = 25.0; // s : FiveM disparu avant que le jeu ait tourné
pub const RELANCES_MAX: u32 = 2;

#[derive(Debug, PartialEq)]
pub enum Action {
    Rien,
    Relancer,
    Fin(&'static str),
}

#[derive(Debug, Default)]
pub struct Suivi {
    pub vu: bool,
    pub jeu_vu: bool,
    pub ouvert: bool,
    pub absent: Option<f64>,
    pub relances: u32,
    pub debut_lancement: f64,
}

impl Suivi {
    /// `deja` : la chasse était déjà en cours (FiveM ouvert au lancement de mousquet).
    pub fn new(deja: bool, t: f64) -> Self {
        Suivi { vu: deja, jeu_vu: deja, debut_lancement: t, ..Default::default() }
    }

    /// Un relevé : les noms des programmes FiveM qui tournent, à l'instant t (s).
    pub fn pas(&mut self, noms: &[String], t: f64) -> Action {
        self.ouvert = !noms.is_empty();
        if self.ouvert {
            self.vu = true;
            self.absent = None;
            self.jeu_vu = self.jeu_vu || noms.iter().any(|n| n.to_lowercase().contains("gtaprocess"));
            return Action::Rien;
        }
        let depuis = t - *self.absent.get_or_insert(t);
        if self.jeu_vu && depuis > ATTENTE_FERMETURE {
            return Action::Fin("FiveM fermé");
        }
        let arrete_au_demarrage = self.vu && !self.jeu_vu && depuis > ATTENTE_DEMARRAGE;
        let jamais_apparu = !self.vu && t - self.debut_lancement > ATTENTE_LANCEMENT;
        if arrete_au_demarrage || jamais_apparu {
            if self.relances < RELANCES_MAX {
                self.relances += 1;
                self.vu = false;
                self.absent = None;
                self.debut_lancement = t;
                return Action::Relancer;
            }
            return Action::Fin(if self.vu { "FiveM ne démarre pas" } else { "FiveM jamais apparu" });
        }
        Action::Rien
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn noms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// Rejoue une suite de (de t0 à t1 : programmes vus) seconde par seconde ; rend les actions non nulles.
    fn rejouer(s: &mut Suivi, etapes: &[(f64, f64, &[&str])]) -> Vec<(f64, Action)> {
        let mut r = vec![];
        for (t0, t1, n) in etapes {
            let mut t = *t0;
            while t < *t1 {
                let a = s.pas(&noms(n), t);
                if a != Action::Rien {
                    let fin = matches!(a, Action::Fin(_));
                    r.push((t, a));
                    if fin {
                        return r;
                    }
                }
                t += 1.0;
            }
        }
        r
    }

    const LANCEUR: &[&str] = &["FiveM.exe"];
    const JEU: &[&str] = &["FiveM.exe", "FiveM_b3751_GTAProcess.exe"];
    const RIEN: &[&str] = &[];

    #[test]
    fn chasse_normale_puis_fermeture() {
        let mut s = Suivi::new(false, 0.0);
        let r = rejouer(&mut s, &[(0.0, 5.0, LANCEUR), (5.0, 600.0, JEU), (600.0, 700.0, RIEN)]);
        assert_eq!(r, vec![(611.0, Action::Fin("FiveM fermé"))]);
    }

    #[test]
    fn fivem_se_relance_seul_en_demarrant_rien_ne_finit() {
        // le lanceur disparaît 8 s (mise à jour) puis revient seul : pas de relance, pas de fin
        let mut s = Suivi::new(false, 0.0);
        let r = rejouer(&mut s, &[(0.0, 10.0, LANCEUR), (10.0, 18.0, RIEN), (18.0, 30.0, LANCEUR), (30.0, 200.0, JEU)]);
        assert!(r.is_empty(), "{r:?}");
    }

    #[test]
    fn fivem_arrete_au_demarrage_est_relance_deux_fois_puis_fin() {
        let mut s = Suivi::new(false, 0.0);
        let r = rejouer(&mut s, &[(0.0, 5.0, LANCEUR), (5.0, 400.0, RIEN)]);
        // disparu à 5 s : relancé après 25 s ; puis jamais revu : relancé après 90 s ; puis fin
        assert_eq!(r[0], (31.0, Action::Relancer));
        assert_eq!(r[1], (122.0, Action::Relancer));
        assert_eq!(r[2], (213.0, Action::Fin("FiveM jamais apparu")));
    }

    #[test]
    fn relance_qui_marche_continue_la_chasse() {
        let mut s = Suivi::new(false, 0.0);
        let r = rejouer(&mut s, &[(0.0, 5.0, LANCEUR), (5.0, 40.0, RIEN), (40.0, 300.0, JEU), (300.0, 320.0, RIEN)]);
        assert_eq!(r, vec![(31.0, Action::Relancer), (311.0, Action::Fin("FiveM fermé"))]);
    }

    #[test]
    fn chasse_deja_en_cours_finit_a_la_fermeture() {
        let mut s = Suivi::new(true, 0.0);
        let r = rejouer(&mut s, &[(0.0, 50.0, JEU), (50.0, 80.0, RIEN)]);
        assert_eq!(r, vec![(61.0, Action::Fin("FiveM fermé"))]);
    }
}
