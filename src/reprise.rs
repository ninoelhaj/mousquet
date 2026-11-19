//! Le filet en cas de coupure de courant ou de PC qui s'éteint en plein jeu : le temps du jeu, une ligne
//! « mousquet_reprise » est ajoutée au démarrage de Windows (HKCU\…\Run). Si le PC redémarre sans que tout ait été
//! remis, mousquet se lance tout seul, sans fenêtre, attend que FiveM soit fermé et remet ton citizen et tes mods.
//! La ligne est retirée dès que tout est revenu : en temps normal, rien ne reste au démarrage.

use std::path::Path;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};
use windows::core::HSTRING;

const CLE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const NOM: &str = "mousquet_reprise";

/// Les essais (MOUSQUET_DOSSIER posé) ne touchent jamais au démarrage de Windows.
fn essai() -> bool {
    std::env::var_os("MOUSQUET_DOSSIER").is_some()
}

/// Ajoute la reprise au démarrage de Windows.
pub fn armer(exe: &Path) -> bool {
    if essai() {
        return true;
    }
    let commande = format!("\"{}\" --reprise", exe.display());
    let w: Vec<u16> = commande.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(CLE),
            &HSTRING::from(NOM),
            REG_SZ.0,
            Some(w.as_ptr() as *const _),
            (w.len() * 2) as u32,
        )
        .is_ok()
    }
}

/// Retire la reprise (tout est revenu).
pub fn desarmer() {
    if essai() {
        return;
    }
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(CLE), &HSTRING::from(NOM));
    }
}
