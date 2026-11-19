//! Les chemins de mousquet et de FiveM. Tout passe par `Chemins` : les essais en donnent un faux, jamais le vrai.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Chemins {
    /// Dossier de mousquet.exe : mods_chasse, reglages, mousquet.log.
    pub ici: PathBuf,
    pub fivem_exe: PathBuf,
    pub fivem_app: PathBuf,
    /// Où tes fichiers attendent pendant le jeu, avec le journal (même disque : déplacements instantanés).
    pub de_cote: PathBuf,
    /// %APPDATA%\CitizenFX\gta5_settings.xml : les réglages graphiques de FiveM.
    pub reglages_jeu: PathBuf,
    /// %APPDATA%\CitizenFX\fivem.cfg : réglages du client (compteur d'images).
    pub fivem_cfg: PathBuf,
    pub reglages: PathBuf,
    pub mods_chasse: PathBuf,
    pub journal: PathBuf,
}

fn env(nom: &str) -> PathBuf {
    std::env::var_os(nom).map(PathBuf::from).unwrap_or_default()
}

impl Chemins {
    /// Les vrais chemins. Le dossier de mousquet est celui de l'exe (ou MOUSQUET_DOSSIER pour le développement).
    pub fn du_systeme() -> Self {
        let ici = std::env::var_os("MOUSQUET_DOSSIER").map(PathBuf::from).unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
        });
        Self::avec(ici, env("LOCALAPPDATA").join("FiveM"), env("APPDATA").join("CitizenFX"))
    }

    /// Chemins construits à partir de trois dossiers (les essais s'en servent avec des dossiers jetables).
    pub fn avec(ici: PathBuf, fivem: PathBuf, citizenfx: PathBuf) -> Self {
        Chemins {
            fivem_exe: fivem.join("FiveM.exe"),
            fivem_app: fivem.join("FiveM.app"),
            de_cote: fivem.join("mis_de_cote_par_mousquet"),
            reglages_jeu: citizenfx.join("gta5_settings.xml"),
            fivem_cfg: citizenfx.join("fivem.cfg"),
            reglages: ici.join("reglages"),
            mods_chasse: ici.join("mods_chasse"),
            journal: ici.join("mousquet.log"),
            ici,
        }
    }

    pub fn liste(&self) -> PathBuf {
        self.de_cote.join("liste.json")
    }
    pub fn reglages_normaux(&self) -> PathBuf {
        self.reglages.join("normal.xml")
    }
    pub fn reglages_chasse(&self) -> PathBuf {
        self.reglages.join("chasse.xml")
    }
    pub fn marque_chasse(&self) -> PathBuf {
        self.reglages.join("EN_CHASSE")
    }
    pub fn fps_garde(&self) -> PathBuf {
        self.reglages.join("fps_origine.txt")
    }
    pub fn macro_erreurs(&self) -> PathBuf {
        self.reglages.join("macro_erreurs")
    }
}
