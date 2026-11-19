//! La remise d'une chasse des anciennes versions de mousquet (Python, ou Rust avant le 18/11 : pack Skyline posé,
//! tes mods et ton pack mis de côté, réglages au plus bas, compteur d'images). Rien n'est plus posé de cette façon ;
//! si une ancienne chasse est restée en plan (crash, coupure), tout revient ici, au lancement suivant.

use crate::chemins::Chemins;
use crate::journal::noter;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::Path;

pub struct Ancien {
    pub ch: Chemins,
    /// « Un programme FiveM tourne-t-il ? » (les essais en donnent un faux).
    pub ouvert: fn() -> bool,
}

impl Ancien {
    fn noter(&self, m: &str) {
        noter(&self.ch, m);
    }

    pub fn en_chasse(&self) -> bool {
        self.ch.liste().exists()
    }

    fn lire_liste(&self) -> Option<Value> {
        serde_json::from_str(&fs::read_to_string(self.ch.liste()).ok()?).ok()
    }

    /// FiveM fermé : enlève les mods et le pack de chasse, remet chaque élément à sa place exacte. Si quelque chose a
    /// pris sa place entre-temps, c'est gardé dans DE_COTE\conflits (rien n'est jamais effacé de tes fichiers).
    pub fn remettre_de_cote(&self) -> io::Result<bool> {
        if !self.en_chasse() {
            return Ok(true);
        }
        if (self.ouvert)() {
            return Ok(false);
        }
        let liste = self.lire_liste().unwrap_or(Value::Null);
        let citizen: Vec<(String, bool)> = liste["citizen"]
            .as_array()
            .map(|a| a.iter().filter_map(|e| Some((e[0].as_str()?.to_string(), e[1].as_bool()?))).collect())
            .unwrap_or_default();
        for (rel, avait) in citizen.iter().rev() {
            let cible = self.ch.fivem_app.join("citizen").join(rel);
            let garde = self.ch.de_cote.join("citizen").join(rel);
            if *avait {
                if !garde.exists() {
                    continue; // jamais déplacé (panne juste avant) : l'original est resté
                }
                if cible.exists() {
                    fs::remove_file(&cible)?; // la copie du pack
                }
                fs::rename(&garde, &cible)?;
            } else if cible.is_file() {
                fs::remove_file(&cible)?; // fichier que seul le pack avait
            }
        }
        let rels = |cle: &str| -> Vec<String> {
            liste[cle].as_array().map(|a| a.iter().filter_map(|r| r.as_str().map(String::from)).collect()).unwrap_or_default()
        };
        for rel in rels("ajoutes") {
            let p = self.ch.fivem_app.join(&rel);
            if p.is_file() {
                fs::remove_file(&p)?; // copie faite par mousquet depuis mods_chasse
            }
        }
        let deplaces = rels("deplaces");
        for rel in &deplaces {
            let (a, b) = (self.ch.fivem_app.join(rel), self.ch.de_cote.join(rel));
            if !b.exists() {
                continue; // jamais déplacé (panne juste avant) : il est resté en place
            }
            if a.exists() {
                let c = self.ch.de_cote.join("conflits").join(rel);
                if let Some(p) = c.parent() {
                    fs::create_dir_all(p)?;
                }
                fs::rename(&a, &c)?;
                self.noter(&format!("chasse : {rel} avait été recréé pendant la chasse, gardé dans conflits"));
            }
            if let Some(p) = a.parent() {
                fs::create_dir_all(p)?;
            }
            fs::rename(&b, &a)?;
        }
        let mut manquants: Vec<String> = deplaces.iter().filter(|r| !self.ch.fivem_app.join(r).exists()).cloned().collect();
        manquants.extend(
            citizen.iter().filter(|(r, avait)| *avait && !self.ch.fivem_app.join("citizen").join(r).exists()).map(|(r, _)| format!("citizen\\{r}")),
        );
        if !manquants.is_empty() {
            self.noter(&format!("ATTENTION chasse : pas revenus : {}", manquants.join(", ")));
            return Ok(false);
        }
        fs::remove_file(self.ch.liste())?;
        supprimer_dossiers_vides(&self.ch.de_cote.join("citizen"));
        for d in ["mods", "plugins"] {
            let _ = fs::remove_dir(self.ch.de_cote.join(d));
        }
        let _ = fs::remove_dir(&self.ch.de_cote);
        self.noter(&format!("chasse finie : tes mods et ton pack sont revenus ({} élément(s))", deplaces.len()));
        Ok(true)
    }

    /// Remet tes réglages (FiveM fermé). Tes retouches faites en chasse sont gardées pour la prochaine fois.
    pub fn remettre_reglages(&self) -> io::Result<bool> {
        if !self.ch.marque_chasse().exists() {
            return Ok(true);
        }
        if (self.ouvert)() {
            return Ok(false);
        }
        fs::copy(&self.ch.reglages_jeu, self.ch.reglages_chasse())?;
        fs::copy(self.ch.reglages_normaux(), &self.ch.reglages_jeu)?;
        fs::remove_file(self.ch.marque_chasse())?;
        self.noter("réglages normaux remis");
        Ok(true)
    }

    /// FiveM fermé : cl_drawFPS reprend sa valeur d'avant la chasse.
    pub fn remettre_fps(&self) -> io::Result<bool> {
        let garde = self.ch.fps_garde();
        if !garde.exists() {
            return Ok(true);
        }
        if (self.ouvert)() {
            return Ok(false);
        }
        let origine = fs::read_to_string(&garde)?.trim().to_string();
        if let Ok(texte) = fs::read_to_string(&self.ch.fivem_cfg) {
            let nouveau = if origine == "absent" {
                Regex::new(r#"(?mi)^seta "cl_drawFPS" "[^"]*"\r?\n?"#).unwrap().replace_all(&texte, "").into_owned()
            } else {
                Regex::new(r#"(?mi)^(seta "cl_drawFPS" ")[^"]*(")"#)
                    .unwrap()
                    .replace_all(&texte, |c: &regex::Captures| format!("{}{origine}{}", &c[1], &c[2]))
                    .into_owned()
            };
            fs::write(&self.ch.fivem_cfg, nouveau)?;
        }
        fs::remove_file(&garde)?;
        self.noter("FPS : compteur de FiveM remis comme avant");
        Ok(true)
    }

    /// Remet tout (FiveM fermé) : mods, pack, compteur FPS, réglages. True si tout est revenu.
    pub fn remettre_tout(&self) -> bool {
        let mut ok = match self.remettre_de_cote() {
            Ok(b) => b,
            Err(e) => {
                self.noter(&format!("remise : {e}"));
                false
            }
        };
        ok &= self.remettre_fps().unwrap_or_else(|e| {
            self.noter(&format!("remise FPS : {e}"));
            false
        });
        ok &= self.remettre_reglages().unwrap_or_else(|e| {
            self.noter(&format!("remise réglages : {e}"));
            false
        });
        ok
    }

    /// Quelque chose reste à remettre (chasse pas finie, réglages ou compteur pas remis).
    pub fn a_remettre(&self) -> bool {
        self.en_chasse() || self.ch.marque_chasse().exists() || self.ch.fps_garde().exists()
    }
}

fn supprimer_dossiers_vides(racine: &Path) {
    let Ok(lecture) = fs::read_dir(racine) else { return };
    for e in lecture.flatten() {
        if e.path().is_dir() {
            supprimer_dossiers_vides(&e.path());
        }
    }
    let _ = fs::remove_dir(racine);
}

// ============================================================================ essais
#[cfg(test)]
mod essais {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn bac(nom: &str) -> (PathBuf, Ancien) {
        let racine = std::env::temp_dir().join(format!("mousquet_ancien_{nom}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&racine);
        let ch = Chemins::avec(racine.join("mousquet"), racine.join("FiveM"), racine.join("CitizenFX"));
        for (rel, data) in [
            ("FiveM.app/mods/herbe.rpf", "herbe"), ("FiveM.app/mods/Sang.rpf", "sang"),
            ("FiveM.app/citizen/common/data/visualsettings.dat", "misc.MoonDimMult 0.3\nautre 1\n"),
        ] {
            let p = racine.join("FiveM").join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, data).unwrap();
        }
        fs::create_dir_all(racine.join("CitizenFX")).unwrap();
        fs::create_dir_all(&ch.reglages).unwrap();
        fs::write(&ch.reglages_jeu, r#"<Settings><Tessellation value="0" /></Settings>"#).unwrap();
        fs::write(ch.reglages_normaux(), r#"<Settings><Tessellation value="3" /></Settings>"#).unwrap();
        fs::write(&ch.fivem_cfg, "// cfg\r\nseta \"cl_drawFPS\" \"true\"\r\n").unwrap();
        (racine, Ancien { ch, ouvert: || false })
    }

    fn liste(c: &Ancien, v: Value) {
        fs::create_dir_all(&c.ch.de_cote).unwrap();
        fs::write(c.ch.liste(), serde_json::to_string(&v).unwrap()).unwrap();
    }

    #[test]
    fn une_ancienne_chasse_restee_en_plan_revient() {
        let (racine, c) = bac("plan");
        // une chasse Skyline : herbe.rpf de côté, lowbush ajouté, visualsettings remplacé, réglages et FPS changés
        fs::create_dir_all(c.ch.de_cote.join("mods")).unwrap();
        fs::rename(c.ch.fivem_app.join("mods/herbe.rpf"), c.ch.de_cote.join("mods/herbe.rpf")).unwrap();
        fs::write(c.ch.fivem_app.join("mods/lowbush.rpf"), "buisson").unwrap();
        let g = c.ch.de_cote.join("citizen/common/data/visualsettings.dat");
        fs::create_dir_all(g.parent().unwrap()).unwrap();
        fs::rename(c.ch.fivem_app.join("citizen/common/data/visualsettings.dat"), &g).unwrap();
        fs::write(c.ch.fivem_app.join("citizen/common/data/visualsettings.dat"), "skyline").unwrap();
        fs::write(c.ch.fivem_app.join("citizen/common/data/w_clear.xml"), "skyline").unwrap();
        liste(&c, json!({"deplaces": ["mods\\herbe.rpf"], "ajoutes": ["mods\\lowbush.rpf"],
            "citizen": [["common\\data\\visualsettings.dat", true], ["common\\data\\w_clear.xml", false]]}));
        fs::write(c.ch.marque_chasse(), "x").unwrap();
        fs::write(c.ch.fps_garde(), "false").unwrap();
        assert!(c.a_remettre() && c.remettre_tout());
        let app = &c.ch.fivem_app;
        assert_eq!(fs::read_to_string(app.join("mods/herbe.rpf")).unwrap(), "herbe");
        assert!(!app.join("mods/lowbush.rpf").exists() && !app.join("citizen/common/data/w_clear.xml").exists());
        assert!(fs::read_to_string(app.join("citizen/common/data/visualsettings.dat")).unwrap().contains("0.3"));
        assert!(fs::read_to_string(&c.ch.reglages_jeu).unwrap().contains(r#"value="3""#));
        assert!(fs::read_to_string(&c.ch.fivem_cfg).unwrap().contains("seta \"cl_drawFPS\" \"false\""));
        assert!(!c.a_remettre() && !c.ch.de_cote.exists());
        let _ = fs::remove_dir_all(racine);
    }

    #[test]
    fn panne_avant_un_deplacement_rien_n_est_efface() {
        let (racine, c) = bac("panne");
        liste(&c, json!({"deplaces": ["mods\\herbe.rpf"], "ajoutes": [], "citizen": [["common\\data\\visualsettings.dat", true]]}));
        assert!(c.remettre_tout());
        assert_eq!(fs::read_to_string(c.ch.fivem_app.join("mods/herbe.rpf")).unwrap(), "herbe");
        assert!(fs::read_to_string(c.ch.fivem_app.join("citizen/common/data/visualsettings.dat")).unwrap().contains("0.3"));
        let _ = fs::remove_dir_all(racine);
    }

    #[test]
    fn conflit_garde_le_fichier_recree() {
        let (racine, c) = bac("conflit");
        fs::create_dir_all(c.ch.de_cote.join("mods")).unwrap();
        fs::copy(c.ch.fivem_app.join("mods/herbe.rpf"), c.ch.de_cote.join("mods/herbe.rpf")).unwrap();
        fs::write(c.ch.fivem_app.join("mods/herbe.rpf"), "recréé par FiveM").unwrap();
        liste(&c, json!({"deplaces": ["mods\\herbe.rpf"], "ajoutes": []}));
        assert!(c.remettre_tout());
        assert_eq!(fs::read_to_string(c.ch.fivem_app.join("mods/herbe.rpf")).unwrap(), "herbe");
        let _ = fs::remove_dir_all(racine);
    }

    #[test]
    fn compatible_avec_la_liste_de_la_version_python() {
        let (racine, c) = bac("python");
        fs::create_dir_all(c.ch.de_cote.join("mods")).unwrap();
        fs::rename(c.ch.fivem_app.join("mods/Sang.rpf"), c.ch.de_cote.join("mods/Sang.rpf")).unwrap();
        fs::write(c.ch.liste(), r#"{"debut": "2026-11-17T08:20:04", "deplaces": ["mods\\Sang.rpf"], "ajoutes": []}"#).unwrap();
        assert!(c.en_chasse() && c.remettre_tout());
        assert!(c.ch.fivem_app.join("mods/Sang.rpf").exists() && !c.en_chasse());
        let _ = fs::remove_dir_all(racine);
    }

    #[test]
    fn rien_ne_bouge_si_fivem_est_ouvert() {
        let (racine, mut c) = bac("ouvert");
        c.ouvert = || true;
        liste(&c, json!({"deplaces": [], "ajoutes": ["mods\\Sang.rpf"]}));
        assert!(!c.remettre_tout());
        assert!(c.ch.fivem_app.join("mods/Sang.rpf").exists());
        let _ = fs::remove_dir_all(racine);
    }
}
