//! La console : une petite fenêtre noire, le nom en grand en rouge, puis des lignes d'état
//! « [ Arbres ]   - sans feuilles » (crochets et nom en bleu, valeur en blanc). Aucune question : elle montre l'état
//! et se ferme toute seule.
//!
//! Double-cliqué, mousquet ouvre sa propre fenêtre (la console classique de Windows, même si Windows Terminal est le
//! terminal par défaut) à sa taille, centrée, sans redimensionnement ni sélection à la souris. Lancé depuis un
//! terminal déjà ouvert, il écrit simplement dedans sans toucher à sa fenêtre.

use std::io::Write;
use std::os::windows::process::CommandExt;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::System::Console::{
    CONSOLE_FONT_INFOEX, CONSOLE_MODE, COORD, ENABLE_EXTENDED_FLAGS, ENABLE_QUICK_EDIT_MODE,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetConsoleProcessList, GetConsoleWindow, GetStdHandle,
    SMALL_RECT, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode, SetConsoleScreenBufferSize, SetConsoleTitleW,
    SetConsoleWindowInfo, SetCurrentConsoleFontEx,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClassNameW, GetWindowLongW, GetWindowRect, SWP_FRAMECHANGED, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongW,
    SetWindowPos, WS_MAXIMIZEBOX, WS_SIZEBOX,
};
use windows::core::HSTRING;

type Rgb = (u8, u8, u8);
// la palette classique de la console Windows (Campbell), en couleurs exactes : même rendu quel que soit le thème
pub const ROUGE: Rgb = (197, 15, 31);
const BLEU: Rgb = (59, 120, 255);
const BLANC: Rgb = (204, 204, 204);
const GRIS: Rgb = (118, 118, 118);

/// La fenêtre : juste la place du nom et des lignes d'état.
const COLONNES: i16 = 75;
const RANGS: i16 = 20;
/// Le nom en grand, dans la police des signatures de console (« d8P », « ?88 », « `?8b »).
const NOM: [&str; 8] = [
    "                                                                d8P",
    "                                                              d88888P",
    " 88bd8b,d8b   d888b  ?88  d8P .d888b,  d88b8P ?88  d8P  d888b  ?88'",
    " 88P'?8P'?8b d8P'?88 d88  88  ?8b,    d8P'?88 d88  88  d8b_,dP 88P",
    "d88  88  88P 88b d88 ?8( d88    `?8b  88b d88 ?8( d88  88b     88b",
    "88'  88' 88b `?888P' `?8P'?8b `?888P' `?8P'88 `?8P'?8b `?888P' `?8b",
    "                                          ?88",
    "                                           88b",
];
const LARGEUR_NOM: usize = 69;
/// Les lignes d'état commencent sous le « o » du nom.
const RETRAIT_ETAT: usize = 12;
/// Les noms des lignes d'état, alignés sur le plus long (« Sécurité »).
const LARGEUR_ETIQUETTE: usize = 8;

fn marge() -> String {
    " ".repeat((COLONNES as usize).saturating_sub(LARGEUR_NOM) / 2)
}

/// Couleur du texte seulement : le fond noir reste.
fn ink(t: &str, c: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m{t}\x1b[39m", c.0, c.1, c.2)
}

fn ecrire(t: &str) {
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(t.as_bytes());
    let _ = o.flush();
}

/// Prépare la console. Rend true si mousquet vient de se relancer dans sa propre fenêtre : ce processus-ci s'arrête.
pub fn preparer() -> bool {
    unsafe {
        let fenetre = GetConsoleWindow();
        let seul = {
            let mut pids = [0u32; 4];
            GetConsoleProcessList(&mut pids) <= 1
        };
        // Windows Terminal (le terminal par défaut de Windows 11) ne se règle pas : relancé dans la console classique
        if seul && classe(fenetre) == "PseudoConsoleWindow" && std::env::var_os("MOUSQUET_CONHOST").is_none() && relancer() {
            return true;
        }
        let _ = SetConsoleTitleW(&HSTRING::from("mousquet"));
        if let Ok(h) = GetStdHandle(STD_OUTPUT_HANDLE) {
            let mut mode = CONSOLE_MODE::default();
            if GetConsoleMode(h, &mut mode).is_ok() {
                let _ = SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
        if !seul {
            return false; // lancé depuis un terminal ouvert : sa fenêtre reste comme elle est
        }
        // un clic dans la fenêtre ne doit pas la figer (sélection de la console)
        if let Ok(h) = GetStdHandle(STD_INPUT_HANDLE) {
            let mut mode = CONSOLE_MODE::default();
            if GetConsoleMode(h, &mut mode).is_ok() {
                let _ = SetConsoleMode(h, (mode & !ENABLE_QUICK_EDIT_MODE) | ENABLE_EXTENDED_FLAGS);
            }
        }
        if classe(fenetre) == "ConsoleWindowClass" {
            regler_fenetre(fenetre);
        }
    }
    // fond noir partout, curseur caché
    ecrire("\x1b[48;2;0;0;0m\x1b[2J\x1b[H\x1b[?25l");
    false
}

/// À la fin : curseur rendu, couleurs normales.
pub fn finir() {
    ecrire("\x1b[0m\x1b[?25h");
}

fn classe(h: HWND) -> String {
    if h.is_invalid() {
        return String::new();
    }
    let mut b = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut b) } as usize;
    String::from_utf16_lossy(&b[..n.min(b.len())])
}

/// mousquet relancé dans la console classique (conhost), avec les mêmes arguments.
fn relancer() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let conhost = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into())
        .join("System32")
        .join("conhost.exe");
    if !conhost.exists() {
        return false;
    }
    std::process::Command::new(conhost)
        .arg(exe)
        .args(std::env::args_os().skip(1))
        .env("MOUSQUET_CONHOST", "1")
        .creation_flags(crate::systeme::DETACHE)
        .spawn()
        .is_ok()
}

/// Police Consolas, fenêtre à la taille du contenu (sans barre de défilement), centrée, taille bloquée.
unsafe fn regler_fenetre(fenetre: HWND) {
    unsafe {
        let Ok(h) = GetStdHandle(STD_OUTPUT_HANDLE) else { return };
        let mut police = CONSOLE_FONT_INFOEX {
            cbSize: size_of::<CONSOLE_FONT_INFOEX>() as u32,
            dwFontSize: COORD { X: 0, Y: 16 },
            FontFamily: 54, // TrueType, chasse fixe
            FontWeight: 400,
            ..Default::default()
        };
        for (i, c) in "Consolas".encode_utf16().enumerate() {
            police.FaceName[i] = c;
        }
        let _ = SetCurrentConsoleFontEx(h, false, &police);
        // fenêtre réduite d'abord, sinon le tampon ne peut pas devenir plus petit qu'elle
        let _ = SetConsoleWindowInfo(h, true, &SMALL_RECT { Left: 0, Top: 0, Right: 0, Bottom: 0 });
        let _ = SetConsoleScreenBufferSize(h, COORD { X: COLONNES, Y: RANGS });
        let _ = SetConsoleWindowInfo(h, true, &SMALL_RECT { Left: 0, Top: 0, Right: COLONNES - 1, Bottom: RANGS - 1 });
        let _ = SetConsoleScreenBufferSize(h, COORD { X: COLONNES, Y: RANGS });
        let style = GetWindowLongW(fenetre, GWL_STYLE);
        SetWindowLongW(fenetre, GWL_STYLE, style & !((WS_MAXIMIZEBOX.0 | WS_SIZEBOX.0) as i32));
        let mut r = RECT::default();
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetWindowRect(fenetre, &mut r).is_ok()
            && GetMonitorInfoW(MonitorFromWindow(fenetre, MONITOR_DEFAULTTONEAREST), &mut info).as_bool()
        {
            let z = info.rcWork;
            let (w, hh) = (r.right - r.left, r.bottom - r.top);
            let (x, y) = (z.left + (z.right - z.left - w) / 2, z.top + (z.bottom - z.top - hh) / 2);
            let _ = SetWindowPos(fenetre, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
        }
    }
}

/// Le nom en grand, en rouge, ligne par ligne.
pub fn banniere() {
    ecrire("\n");
    let m = marge();
    for ligne in NOM {
        ecrire(&format!("{m}{}\n", ink(ligne, ROUGE)));
        std::thread::sleep(std::time::Duration::from_millis(18));
    }
    ecrire("\n");
}

/// « [ Arbres ]   - sans feuilles » : comme une signature de console, le tiret aligné sur le nom le plus long.
pub fn statut(nom: &str, valeur: &str, couleur: Rgb) {
    let pad = " ".repeat(LARGEUR_ETIQUETTE.saturating_sub(nom.chars().count()));
    let retrait = " ".repeat(marge().len() + RETRAIT_ETAT);
    ecrire(&format!(
        "{retrait}{}{}{} {}{pad} {} {}\n",
        ink("[", BLEU),
        ink(" ", BLEU),
        ink(nom, BLEU),
        ink("]", BLEU),
        ink("-", BLANC),
        ink(valeur, couleur)
    ));
    std::thread::sleep(std::time::Duration::from_millis(35));
}

pub fn ok(nom: &str, valeur: &str) {
    statut(nom, valeur, BLANC);
}

/// Une ou deux lignes grises en bas (coupées sur « · »).
pub fn pied(t: &str) {
    let retrait = " ".repeat(marge().len() + RETRAIT_ETAT);
    ecrire("\n");
    for morceau in t.split(" · ") {
        ecrire(&format!("{retrait}{}\n", ink(morceau, GRIS)));
    }
}
