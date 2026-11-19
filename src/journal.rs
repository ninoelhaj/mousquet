//! mousquet.log : une ligne datée par événement. Une erreur d'écriture ne doit jamais arrêter la chasse.

use crate::chemins::Chemins;
use std::io::Write;

/// « 2026-11-18 08:01:37 », à l'heure de l'ordinateur.
pub fn horodatage() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

pub fn noter(ch: &Chemins, message: &str) {
    noter_dans(&ch.journal, message);
}

pub fn noter_dans(journal: &std::path::Path, message: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(journal) {
        let _ = writeln!(f, "{}  {}", horodatage(), message);
    }
}
