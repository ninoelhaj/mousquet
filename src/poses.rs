//! Ce que mousquet pose dans FiveM le temps du jeu : les arbres sans feuilles (20 réglages de shaders vides dans
//! FiveM.app\citizen\common\shaders\db, ceux du pack Skyline) et les mods de mods_chasse (lowbush). Quand FiveM se
//! ferme, tout est retiré : ton citizen et tes mods redeviennent exactement ce qu'ils étaient.
//!
//! Sécurités, pour qu'un crash ou une coupure de courant ne laisse jamais ton citizen abîmé :
//! - le journal (ce qui va être posé, ce qui existait, les dossiers qui seront créés) est écrit AVANT de toucher à quoi
//!   que ce soit, et forcé sur le disque ;
//! - un fichier à toi n'est jamais écrasé : il est déplacé dans mis_de_cote_par_mousquet\poses, puis remis ;
//! - chaque fichier posé est écrit à côté puis renommé : jamais de fichier à moitié écrit ;
//! - au retrait, un fichier n'est effacé que s'il est exactement celui que mousquet a posé (taille et empreinte) ;
//!   un fichier qui aurait changé entre-temps est gardé dans mis_de_cote_par_mousquet\conflits ;
//! - les déplacements attendent l'écriture sur le disque (MOVEFILE_WRITE_THROUGH) et réessaient si un antivirus
//!   tient le fichier ;
//! - si quoi que ce soit manque, le journal reste : la remise sera refaite au prochain lancement.

use serde_json::{Value, json};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};
use windows::core::HSTRING;

/// Les réglages de shaders des arbres, vidés comme dans le pack Skyline : les feuillages ne sont plus dessinés.
pub const ARBRES: [&str; 20] = [
    "gta_trees", "trees", "trees_camera_aligned", "trees_camera_facing", "trees_lod", "trees_lod2", "trees_lod2d",
    "trees_lod_tnt", "trees_normal", "trees_normal_diffspec", "trees_normal_diffspec_tnt", "trees_normal_spec",
    "trees_normal_spec_camera_aligned", "trees_normal_spec_camera_aligned_tnt", "trees_normal_spec_camera_facing",
    "trees_normal_spec_camera_facing_tnt", "trees_normal_spec_tnt", "trees_normal_spec_wind", "trees_shadow_proxy",
    "trees_tnt",
];

/// Un fichier à poser, relatif à FiveM.app (« citizen\common\shaders\db\trees.sps », « mods\lowbush.rpf »).
pub struct Pose {
    pub rel: String,
    pub contenu: Vec<u8>,
}

pub fn arbres() -> Vec<Pose> {
    ARBRES
        .iter()
        .map(|n| Pose {
            rel: format!("citizen\\common\\shaders\\db\\{n}.sps"),
            contenu: if *n == "trees_lod" { b"\r\n".to_vec() } else { Vec::new() },
        })
        .collect()
}

/// Les .rpf de mods_chasse (jamais de programme : .dll, .asi… ne sont pas pris).
pub fn mods(dossier: &Path) -> Vec<Pose> {
    let mut v: Vec<Pose> = fs::read_dir(dossier)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().is_file() && e.file_name().to_string_lossy().to_lowercase().ends_with(".rpf"))
                .filter_map(|e| Some(Pose { rel: format!("mods\\{}", e.file_name().to_string_lossy()), contenu: fs::read(e.path()).ok()? }))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| a.rel.cmp(&b.rel));
    v
}

/// Empreinte FNV-1a 64 bits : reconnaître un fichier posé par mousquet.
pub fn empreinte(d: &[u8]) -> u64 {
    d.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100_0000_01b3))
}

/// Réessaie quelques fois (un antivirus ou l'indexation de Windows peut tenir un fichier un instant).
fn reessayer<T>(mut f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut derniere = None;
    for i in 0..6 {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(e),
            Err(e) => derniere = Some(e),
        }
        std::thread::sleep(Duration::from_millis(100 * (i + 1)));
    }
    Err(derniere.unwrap_or_else(|| io::Error::other("échec")))
}

/// Déplace (même disque : instantané), en attendant que ce soit écrit sur le disque.
fn deplacer(de: &Path, vers: &Path) -> io::Result<()> {
    reessayer(|| unsafe {
        MoveFileExW(&HSTRING::from(de.as_os_str()), &HSTRING::from(vers.as_os_str()), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
    })
}

/// Écrit un fichier entier à côté, le force sur le disque, puis le met à sa place : jamais à moitié écrit.
fn ecrire_sur(chemin: &Path, d: &[u8]) -> io::Result<()> {
    let tmp = temporaire(chemin);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(d)?;
        f.sync_all()?;
    }
    deplacer(&tmp, chemin)
}

fn temporaire(chemin: &Path) -> PathBuf {
    let mut s = chemin.as_os_str().to_owned();
    s.push(".mousquet_tmp");
    PathBuf::from(s)
}

fn effacer(p: &Path) -> io::Result<()> {
    reessayer(|| fs::remove_file(p))
}

pub struct Poses {
    /// FiveM.app
    pub app: PathBuf,
    /// mis_de_cote_par_mousquet
    pub de_cote: PathBuf,
    /// mousquet.log
    pub log: PathBuf,
    /// « Un programme FiveM tourne-t-il ? » (les essais en donnent un faux).
    pub ouvert: fn() -> bool,
}

impl Poses {
    pub fn journal(&self) -> PathBuf {
        self.de_cote.join("poses.json")
    }

    fn garde(&self) -> PathBuf {
        self.de_cote.join("poses")
    }

    /// Des poses sont en place (jeu en cours, ou pas encore remises après un crash).
    pub fn actives(&self) -> bool {
        self.journal().exists()
    }

    fn ecrire_journal(&self, v: &Value) -> io::Result<()> {
        fs::create_dir_all(&self.de_cote)?;
        ecrire_sur(&self.journal(), serde_json::to_string_pretty(v).unwrap_or_default().as_bytes())
    }

    /// Pose les fichiers (FiveM fermé). En cas d'erreur, appeler `remettre` : il défait ce qui a été fait.
    pub fn poser(&self, poses: &[Pose]) -> io::Result<usize> {
        if (self.ouvert)() {
            return Err(io::Error::other("FiveM est ouvert"));
        }
        if self.actives() {
            return Err(io::Error::other("des poses d'avant ne sont pas remises"));
        }
        // 1. le plan complet, sur le disque, avant de toucher à quoi que ce soit
        let mut dossiers: Vec<PathBuf> = Vec::new();
        for p in poses {
            let mut d = self.app.join(&p.rel).parent().map(Path::to_path_buf);
            while let Some(x) = d {
                if x.exists() || x == self.app || dossiers.contains(&x) {
                    break;
                }
                dossiers.push(x.clone());
                d = x.parent().map(Path::to_path_buf);
            }
        }
        let fichiers: Vec<Value> = poses
            .iter()
            .map(|p| json!({"rel": p.rel, "avait": self.app.join(&p.rel).exists(), "taille": p.contenu.len(), "empreinte": format!("{:016x}", empreinte(&p.contenu))}))
            .collect();
        let crees: Vec<String> = dossiers.iter().filter_map(|d| d.strip_prefix(&self.app).ok()).map(|d| d.to_string_lossy().to_string()).collect();
        self.ecrire_journal(&json!({"version": 1, "debut": crate::journal::horodatage(), "fichiers": fichiers, "dossiers_crees": crees}))?;
        // 2. les gestes, dans l'ordre du plan
        for (p, f) in poses.iter().zip(&fichiers) {
            let cible = self.app.join(&p.rel);
            if f["avait"].as_bool() == Some(true) {
                let garde = self.garde().join(&p.rel);
                fs::create_dir_all(garde.parent().unwrap_or(&self.de_cote))?;
                deplacer(&cible, &garde)?;
            }
            fs::create_dir_all(cible.parent().unwrap_or(&self.app))?;
            ecrire_sur(&cible, &p.contenu)?;
        }
        Ok(poses.len())
    }

    /// Retire ce qui a été posé et remet tes fichiers (FiveM fermé). True si tout est revenu (ou rien à faire).
    pub fn remettre(&self) -> io::Result<bool> {
        if !self.actives() {
            return Ok(true);
        }
        if (self.ouvert)() {
            return Ok(false);
        }
        let texte = fs::read_to_string(self.journal())?;
        let j: Value = serde_json::from_str(&texte).map_err(|e| io::Error::other(format!("journal illisible : {e}")))?;
        let fichiers = j["fichiers"].as_array().cloned().unwrap_or_default();
        let mut manquants = Vec::new();
        for f in fichiers.iter().rev() {
            let Some(rel) = f["rel"].as_str() else { continue };
            let avait = f["avait"].as_bool().unwrap_or(false);
            let cible = self.app.join(rel);
            let tmp = temporaire(&cible);
            if tmp.exists() {
                effacer(&tmp)?; // coupure pendant une écriture : le morceau à côté part
            }
            let notre = cible.is_file()
                && fs::metadata(&cible).map(|m| Some(m.len()) == f["taille"].as_u64()).unwrap_or(false)
                && fs::read(&cible).map(|d| Some(format!("{:016x}", empreinte(&d)).as_str()) == f["empreinte"].as_str()).unwrap_or(false);
            let garde = self.garde().join(rel);
            if avait {
                if !garde.exists() {
                    if !cible.exists() {
                        manquants.push(rel.to_string());
                    }
                    continue; // jamais déplacé (coupure juste avant) : ton fichier est resté à sa place
                }
                if cible.exists() {
                    if notre {
                        effacer(&cible)?;
                    } else {
                        self.au_conflit(&cible, rel)?;
                    }
                }
                deplacer(&garde, &cible)?;
            } else if notre {
                effacer(&cible)?;
            } else if cible.exists() {
                crate::journal::noter_dans(&self.log, &format!("poses : {rel} a changé pendant le jeu, laissé en place"));
            }
        }
        if !manquants.is_empty() {
            return Err(io::Error::other(format!("pas revenus : {}", manquants.join(", "))));
        }
        // les dossiers que mousquet avait créés, s'ils sont vides (du plus profond au moins profond)
        let mut crees: Vec<String> = j["dossiers_crees"].as_array().map(|a| a.iter().filter_map(|d| d.as_str().map(String::from)).collect()).unwrap_or_default();
        crees.sort_by_key(|d| std::cmp::Reverse(d.len()));
        for d in crees {
            let _ = fs::remove_dir(self.app.join(d));
        }
        effacer(&self.journal())?;
        supprimer_dossiers_vides(&self.garde());
        let _ = fs::remove_dir(&self.de_cote);
        Ok(true)
    }

    /// Un fichier qui n'est plus celui posé par mousquet : gardé à part, jamais effacé.
    fn au_conflit(&self, cible: &Path, rel: &str) -> io::Result<()> {
        let c = self.de_cote.join("conflits").join(rel);
        fs::create_dir_all(c.parent().unwrap_or(&self.de_cote))?;
        deplacer(cible, &c)?;
        crate::journal::noter_dans(&self.log, &format!("poses : {rel} avait changé, gardé dans conflits"));
        Ok(())
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
    use std::collections::BTreeMap;

    fn bac(nom: &str) -> (PathBuf, Poses) {
        let r = std::env::temp_dir().join(format!("mousquet_poses_{nom}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&r);
        for (rel, d) in [
            ("FiveM.app/citizen/common/data/visualsettings.dat", "mon pack"),
            ("FiveM.app/citizen/common/data/timecycle/rshiiiiiii.xml", "ma lumière"),
            ("FiveM.app/mods/mon_mod.rpf", "à moi"),
            ("FiveM.app/mods/lowbush.rpf", "mon vieux lowbush"),
        ] {
            let p = r.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, d).unwrap();
        }
        let p = Poses { app: r.join("FiveM.app"), de_cote: r.join("mis_de_cote_par_mousquet"), log: r.join("mousquet.log"), ouvert: || false };
        (r, p)
    }

    fn photo(d: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut m = BTreeMap::new();
        let mut pile = vec![d.to_path_buf()];
        while let Some(x) = pile.pop() {
            for e in fs::read_dir(&x).unwrap().flatten() {
                let rel = e.path().strip_prefix(d).unwrap().to_string_lossy().to_string();
                if e.path().is_dir() {
                    m.insert(format!("{rel}\\"), Vec::new()); // les dossiers comptent aussi
                    pile.push(e.path());
                } else {
                    m.insert(rel, fs::read(e.path()).unwrap());
                }
            }
        }
        m
    }

    fn a_poser() -> Vec<Pose> {
        let mut v = arbres();
        v.push(Pose { rel: "mods\\lowbush.rpf".into(), contenu: b"buisson".to_vec() });
        v
    }

    #[test]
    fn pose_puis_tout_revient_a_l_identique() {
        let (r, p) = bac("aller_retour");
        let avant = photo(&r.join("FiveM.app"));
        assert_eq!(p.poser(&a_poser()).unwrap(), 21);
        let db = p.app.join("citizen/common/shaders/db");
        assert_eq!(fs::read_dir(&db).unwrap().count(), 20);
        assert_eq!(fs::read(db.join("trees_lod.sps")).unwrap(), b"\r\n");
        assert_eq!(fs::read(p.app.join("mods/lowbush.rpf")).unwrap(), b"buisson");
        assert_eq!(fs::read(p.app.join("mods/mon_mod.rpf")).unwrap(), "à moi".as_bytes(), "tes mods restent");
        assert!(p.remettre().unwrap());
        assert_eq!(photo(&r.join("FiveM.app")), avant, "FiveM.app revenu à l'identique, dossiers compris");
        assert!(!p.de_cote.exists());
        let _ = fs::remove_dir_all(r);
    }

    /// Coupure de courant après N gestes : la remise ramène toujours tout, quel que soit le moment.
    #[test]
    fn coupure_a_chaque_instant() {
        for n in 0..=22 {
            let (r, p) = bac(&format!("coupure_{n}"));
            let avant = photo(&r.join("FiveM.app"));
            let poses = a_poser();
            // le journal complet est écrit, puis seulement les n premiers gestes
            let fichiers: Vec<Value> = poses
                .iter()
                .map(|x| json!({"rel": x.rel, "avait": p.app.join(&x.rel).exists(), "taille": x.contenu.len(), "empreinte": format!("{:016x}", empreinte(&x.contenu))}))
                .collect();
            p.ecrire_journal(&json!({"version": 1, "fichiers": fichiers, "dossiers_crees": ["citizen\\common\\shaders\\db", "citizen\\common\\shaders"]})).unwrap();
            for (x, f) in poses.iter().zip(&fichiers).take(n) {
                let cible = p.app.join(&x.rel);
                if f["avait"].as_bool() == Some(true) {
                    let g = p.garde().join(&x.rel);
                    fs::create_dir_all(g.parent().unwrap()).unwrap();
                    deplacer(&cible, &g).unwrap();
                }
                fs::create_dir_all(cible.parent().unwrap()).unwrap();
                fs::write(&cible, &x.contenu).unwrap();
            }
            if n == 22 {
                // coupure pendant l'écriture d'un fichier : un morceau reste à côté
                fs::write(temporaire(&p.app.join("mods/lowbush.rpf")), b"bui").unwrap();
            }
            assert!(p.remettre().unwrap(), "coupure après {n} gestes");
            assert_eq!(photo(&r.join("FiveM.app")), avant, "coupure après {n} gestes");
            let _ = fs::remove_dir_all(r);
        }
    }

    #[test]
    fn rien_ne_bouge_si_fivem_est_ouvert() {
        let (r, mut p) = bac("ouvert");
        p.ouvert = || true;
        let avant = photo(&r.join("FiveM.app"));
        assert!(p.poser(&a_poser()).is_err());
        assert_eq!(photo(&r.join("FiveM.app")), avant);
        let _ = fs::remove_dir_all(r);
    }

    #[test]
    fn un_fichier_change_pendant_le_jeu_n_est_jamais_efface() {
        let (r, p) = bac("change");
        p.poser(&a_poser()).unwrap();
        fs::write(p.app.join("mods/lowbush.rpf"), "remplacé pendant le jeu").unwrap();
        fs::write(p.app.join("citizen/common/shaders/db/trees.sps"), "réglage à toi").unwrap();
        assert!(p.remettre().unwrap());
        assert_eq!(fs::read_to_string(p.app.join("mods/lowbush.rpf")).unwrap(), "mon vieux lowbush", "le tien revient");
        assert_eq!(fs::read_to_string(p.de_cote.join("conflits/mods/lowbush.rpf")).unwrap(), "remplacé pendant le jeu");
        assert_eq!(fs::read_to_string(p.app.join("citizen/common/shaders/db/trees.sps")).unwrap(), "réglage à toi", "laissé");
        let _ = fs::remove_dir_all(r);
    }

    #[test]
    fn deux_poses_de_suite_refusees_tant_que_rien_n_est_remis() {
        let (r, p) = bac("double");
        p.poser(&arbres()).unwrap();
        assert!(p.poser(&arbres()).is_err());
        assert!(p.remettre().unwrap() && !p.actives());
        let _ = fs::remove_dir_all(r);
    }
}
