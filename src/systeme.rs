//! Ce que mousquet demande à Windows : les programmes FiveM qui tournent, la fenêtre du jeu, le lancement de FiveM
//! et du gardien. Rien n'est lu dans le jeu : seulement des noms de programmes et des positions de fenêtres.

use std::os::windows::process::CommandExt;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId,
};
use windows::core::PWSTR;

pub const DETACHE: u32 = 0x0000_0008 | 0x0000_0200; // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
pub const SANS_FENETRE: u32 = 0x0800_0000; // CREATE_NO_WINDOW

fn texte_utf16(b: &[u16]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..n])
}

/// Noms des programmes FiveM qui tournent (le lanceur, le jeu, son navigateur…).
pub fn processus_fivem() -> Vec<String> {
    let mut noms = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return noms };
        let mut pe = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut pe).is_ok();
        while ok {
            let nom = texte_utf16(&pe.szExeFile);
            if nom.to_lowercase().starts_with("fivem") {
                noms.push(nom);
            }
            ok = Process32NextW(snap, &mut pe).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    noms
}

pub fn fivem_ouvert() -> bool {
    !processus_fivem().is_empty()
}

/// Nom de l'exe d'une fenêtre, en minuscules (« fivem_b3751_gtaprocess.exe »).
pub fn exe_de_fenetre(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n);
        let _ = CloseHandle(h);
        if r.is_err() {
            return String::new();
        }
        let chemin = String::from_utf16_lossy(&buf[..n as usize]);
        Path::new(&chemin).file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default()
    }
}

fn est_le_jeu(exe: &str) -> bool {
    exe.starts_with("fivem") && exe.contains("gtaprocess")
}

/// La fenêtre du jeu si elle est au premier plan.
pub fn jeu_devant() -> Option<HWND> {
    let h = unsafe { GetForegroundWindow() };
    (!h.is_invalid() && est_le_jeu(&exe_de_fenetre(h))).then_some(h)
}

pub fn lancer_fivem(exe: &Path) -> std::io::Result<()> {
    std::process::Command::new(exe)
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .creation_flags(DETACHE)
        .spawn()
        .map(|_| ())
}

/// Le programme de fond : ce même exe, détaché et sans fenêtre (macro, suivi de FiveM, tout remis à sa fermeture).
pub fn lancer_fond(dossier: &Path, deja: bool) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("--fond")
        .arg(if deja { "1" } else { "0" })
        .current_dir(dossier)
        .creation_flags(DETACHE | SANS_FENETRE)
        .spawn()
        .map(|_| ())
}

/// Le gardien : ce même exe, détaché et sans fenêtre, qui remettra tout à la fermeture de FiveM.
pub fn lancer_gardien(dossier: &Path) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("--gardien")
            .arg(std::process::id().to_string())
            .current_dir(dossier)
            .creation_flags(DETACHE | SANS_FENETRE)
            .spawn();
    }
}
