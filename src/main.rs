//! mousquet : pour la chasse à Cayo Perico (FiveM, Baylife).
//!
//! Double-clic, et c'est tout : les arbres sans feuilles et lowbush sont posés dans ton FiveM (ton pack et tes mods
//! restent), FiveM est lancé, la console montre l'état et se ferme. Un programme de fond, sans fenêtre, garde la
//! macro peaux (Ctrl+F7), relance FiveM s'il s'arrête en démarrant, et retire tout quand FiveM se ferme. Après un
//! crash ou une coupure, tout revient au prochain démarrage de Windows ou au prochain lancement de mousquet.
mod ancien;
mod chemins;
mod console;
mod journal;
mod lecture;
mod macro_peaux;
mod poses;
mod raccourcis;
mod reprise;
mod suivi;
mod systeme;

use ancien::Ancien;
use chemins::Chemins;
use console::{ROUGE, ok, statut};
use journal::noter;
use poses::Poses;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Console::{FreeConsole, SetConsoleCtrlHandler};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, OpenMutexW, OpenProcess, PROCESS_SYNCHRONIZE,
    SYNCHRONIZATION_SYNCHRONIZE, SetEvent, WaitForSingleObject,
};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::core::{BOOL, HSTRING};

/// Tenu par le programme de fond : une console relancée voit que mousquet est déjà actif.
const FOND: &str = "Local\\MousquetFond";
/// Tenu aussi par le programme de fond ; les anciennes versions se ferment quand on le déclenche, et leurs gardiens
/// s'effacent tant qu'il existe.
const EVENEMENT: &str = "Local\\MousquetStop";

static CHEMINS: OnceLock<Chemins> = OnceLock::new();

fn ch() -> &'static Chemins {
    CHEMINS.get().expect("chemins")
}

fn poses() -> Poses {
    let c = ch();
    Poses { app: c.fivem_app.clone(), de_cote: c.de_cote.clone(), log: c.journal.clone(), ouvert: systeme::fivem_ouvert }
}

fn ancien() -> Ancien {
    Ancien { ch: ch().clone(), ouvert: systeme::fivem_ouvert }
}

/// Quelque chose reste à remettre (poses de cette version, ou une chasse d'une ancienne version).
fn a_remettre() -> bool {
    poses().actives() || ancien().a_remettre()
}

/// Remet tout (FiveM fermé). True si tout est revenu.
fn tout_remettre() -> bool {
    let anc = ancien().remettre_tout();
    let p = match poses().remettre() {
        Ok(b) => b,
        Err(e) => {
            noter(ch(), &format!("remise : {e}"));
            false
        }
    };
    anc && p
}

/// Les essais (MOUSQUET_DOSSIER posé) ont leurs propres noms : ils ne gênent jamais un vrai mousquet.
fn nom_reel(base: &str) -> HSTRING {
    HSTRING::from(if std::env::var_os("MOUSQUET_DOSSIER").is_some() { format!("{base}Essai") } else { base.to_string() })
}

fn existe_mutex(nom: &str) -> bool {
    unsafe {
        OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &nom_reel(nom)).map(|h| CloseHandle(h)).is_ok()
    }
}

fn existe_evenement() -> bool {
    unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &nom_reel(EVENEMENT)).map(|h| CloseHandle(h)).is_ok() }
}

fn main() {
    unsafe {
        // pixels réels de l'écran : la capture des menus tombe juste, même avec un zoom de Windows
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let _ = CHEMINS.set(Chemins::du_systeme());
    let args: Vec<String> = std::env::args().collect();
    let arg = |nom: &str| args.iter().position(|a| a == nom);
    if let Some(i) = arg("--fond") {
        fond(args.get(i + 1).is_some_and(|v| v == "1"));
        return;
    }
    if let Some(i) = arg("--gardien") {
        veiller(args.get(i + 1).and_then(|p| p.parse().ok()));
        return;
    }
    if arg("--reprise").is_some() {
        // lancé par Windows au démarrage, après une coupure : sans fenêtre
        unsafe {
            let _ = FreeConsole();
        }
        noter(ch(), "reprise au démarrage de Windows : remise de ce qui était resté posé");
        veiller(None);
        return;
    }
    if console::preparer() {
        return; // relancé dans sa propre petite fenêtre
    }
    if arg("--apercu").is_some() {
        // la console seule, avec un état d'exemple : rien n'est posé ni lancé
        console::banniere();
        ok("FiveM", "lancé");
        ok("Arbres", "sans feuilles");
        ok("Mods", "lowbush");
        ok("Macro", "ctrl+F7");
        ok("Sécurité", "journal + reprise au démarrage de Windows");
        console::pied("Tout revient quand FiveM se ferme, même après un crash. · Cette fenêtre se ferme toute seule.");
        std::thread::sleep(Duration::from_secs(12));
        console::finir();
        return;
    }
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(croix), true);
    }
    let r = console();
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(croix), false);
    }
    match r {
        Ok(()) => std::thread::sleep(Duration::from_secs(5)),
        Err(ex) => {
            noter(ch(), &format!("ERREUR {ex}"));
            statut("Erreur", &ex, ROUGE);
            console::pied("Rien n'est resté posé. · Détail dans mousquet.log.");
            std::thread::sleep(Duration::from_secs(15));
        }
    }
    console::finir();
}

// ============================================================================ la console
/// Fenêtre fermée par la croix avant que le programme de fond ait pris la main : un gardien remet tout.
unsafe extern "system" fn croix(evenement: u32) -> BOOL {
    if matches!(evenement, 2 | 5 | 6) && !existe_mutex(FOND) && a_remettre() {
        systeme::lancer_gardien(&ch().ici);
    }
    BOOL(0)
}

/// « Actif » seulement quand le programme de fond tourne vraiment.
fn attendre_fond() -> Result<(), String> {
    let fin = Instant::now() + Duration::from_secs(5);
    while Instant::now() < fin {
        if existe_mutex(FOND) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("le programme de fond n'a pas démarré".into())
}

fn console() -> Result<(), String> {
    let c = ch();
    console::banniere();
    if existe_mutex(FOND) {
        ok("mousquet", "déjà actif");
        ok("Retour", "tout revient à la fermeture de FiveM");
        return Ok(());
    }
    if !c.fivem_exe.exists() {
        return Err("FiveM introuvable".into());
    }
    // une ancienne version encore ouverte (console Python ou Rust) se ferme : le programme de fond prend la suite
    if existe_evenement() {
        unsafe {
            if let Ok(h) = OpenEventW(EVENT_MODIFY_STATE, false, &nom_reel(EVENEMENT)) {
                let _ = SetEvent(h);
                let _ = CloseHandle(h);
            }
        }
        std::thread::sleep(Duration::from_millis(1500));
    }
    let _ = std::fs::create_dir_all(&c.mods_chasse);

    if systeme::fivem_ouvert() {
        // le jeu lit ces fichiers à son lancement : rien n'est posé maintenant, la macro marche quand même
        let posees = a_remettre();
        systeme::lancer_fond(&c.ici, true).map_err(|x| format!("programme de fond : {x}"))?;
        attendre_fond()?;
        ok("FiveM", "déjà ouvert");
        ok("Arbres", if posees { "sans feuilles" } else { "au prochain lancement de FiveM" });
        ok("Macro", "ctrl+F7");
        console::pied("Tout revient quand FiveM se ferme. · Cette fenêtre se ferme toute seule.");
        return Ok(());
    }

    // ce qui serait resté d'avant (crash, coupure) revient d'abord
    if !tout_remettre() {
        return Err("la remise d'avant n'a pas pu se faire · voir mousquet.log".into());
    }
    let mods = poses::mods(&c.mods_chasse);
    let noms: Vec<String> = mods.iter().map(|m| m.rel.trim_start_matches("mods\\").trim_end_matches(".rpf").to_string()).collect();
    let mut a_poser = poses::arbres();
    a_poser.extend(mods);
    let exe = std::env::current_exe().map_err(|x| format!("mousquet.exe introuvable : {x}"))?;
    let reprise = reprise::armer(&exe);
    let p = poses();
    let defaire = |x: String| -> Result<(), String> {
        if !tout_remettre() {
            noter(c, "remise après l'erreur incomplète : elle sera refaite au prochain lancement");
        } else {
            reprise::desarmer();
        }
        Err(x)
    };
    if let Err(x) = p.poser(&a_poser) {
        return defaire(format!("pose : {x}"));
    }
    noter(c, &format!("posé : arbres sans feuilles ({}) · mods : {}", poses::ARBRES.len(),
        if noms.is_empty() { "aucun".to_string() } else { noms.join(", ") }));
    if let Err(x) = systeme::lancer_fivem(&c.fivem_exe) {
        return defaire(format!("lancement de FiveM : {x}"));
    }
    noter(c, "FiveM lancé");
    if let Err(x) = systeme::lancer_fond(&c.ici, false).map_err(|x| format!("programme de fond : {x}")).and_then(|_| attendre_fond()) {
        systeme::lancer_gardien(&c.ici); // tout reviendra quand même à la fermeture de FiveM
        return Err(x);
    }
    ok("FiveM", "lancé");
    ok("Arbres", "sans feuilles");
    ok("Mods", &if noms.is_empty() { "aucun dans mods_chasse".to_string() } else { noms.join(", ") });
    ok("Macro", "ctrl+F7");
    ok("Sécurité", if reprise { "journal + reprise au démarrage de Windows" } else { "journal (reprise au démarrage refusée)" });
    console::pied("Tout revient quand FiveM se ferme, même après un crash. · Cette fenêtre se ferme toute seule.");
    Ok(())
}

// ============================================================================ le programme de fond
/// Sans fenêtre : la macro, le suivi de FiveM, puis tout remis à sa fermeture.
fn fond(deja: bool) {
    let c = ch();
    let (verrou, evenement) = unsafe {
        let v = CreateMutexW(None, true, &nom_reel(FOND)).unwrap_or_default();
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(v);
            return; // déjà un programme de fond
        }
        (v, CreateEventW(None, true, false, &nom_reel(EVENEMENT)).unwrap_or_default())
    };
    noter(c, "programme de fond en route · macro ctrl+F7");
    let fil = raccourcis::Raccourcis::demarrer(c.clone(), true);
    let debut = Instant::now();
    let mut suivi = suivi::Suivi::new(deja, 0.0);
    let mut vus = std::collections::BTreeSet::new();
    let raison = loop {
        let noms = systeme::processus_fivem();
        vus.extend(noms.iter().cloned());
        match suivi.pas(&noms, debut.elapsed().as_secs_f64()) {
            suivi::Action::Rien => {}
            suivi::Action::Relancer => {
                // FiveM s'est arrêté avant que le jeu tourne (mise à jour, changement de version…) : relancé ici
                let v: Vec<String> = vus.iter().cloned().collect();
                noter(c, &format!("FiveM arrêté pendant son démarrage (vus : {}) : relancé ({}/{})",
                    if v.is_empty() { "aucun".into() } else { v.join(", ") }, suivi.relances, suivi::RELANCES_MAX));
                let _ = systeme::lancer_fivem(&c.fivem_exe);
            }
            suivi::Action::Fin(r) => break r,
        }
        std::thread::sleep(Duration::from_secs(1));
    };
    fil.arreter();
    let v: Vec<String> = vus.iter().cloned().collect();
    noter(c, &format!("FiveM : vus pendant le jeu : {} · jeu vu : {} · relances : {}",
        if v.is_empty() { "aucun".into() } else { v.join(", ") }, if suivi.jeu_vu { "oui" } else { "non" }, suivi.relances));
    // FiveM rouvert entre-temps : on attend sa fermeture pour tout remettre
    while !tout_remettre() {
        std::thread::sleep(Duration::from_secs(3));
    }
    reprise::desarmer();
    noter(c, &format!("fin ({raison}) · ton citizen et tes mods sont revenus"));
    unsafe {
        let _ = CloseHandle(evenement);
        let _ = CloseHandle(verrou);
    }
}

/// Gardien (console fermée trop tôt) et reprise (démarrage de Windows après une coupure) : attend que FiveM soit
/// fermé, remet tout, retire la reprise du démarrage. S'efface si un programme de fond s'en occupe.
fn veiller(parent: Option<u32>) {
    if let Some(pid) = parent {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
                WaitForSingleObject(h, 30_000);
                let _ = CloseHandle(h);
            }
        }
    }
    loop {
        if existe_mutex(FOND) {
            return;
        }
        if !a_remettre() {
            reprise::desarmer();
            return;
        }
        if tout_remettre() {
            reprise::desarmer();
            noter(ch(), "tout est revenu (après une fermeture ou une coupure)");
            return;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}
