//! La macro peaux au clavier : Ctrl+F7, dans un fil à part du programme de fond, seulement si tu as dit oui.
//! Chaque résultat et chaque arrêt vont dans mousquet.log.

use crate::chemins::Chemins;
use crate::journal::noter;
use crate::macro_peaux::{self, Arret, JeuReel, MACRO_NOM, MACRO_VK};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, WM_HOTKEY, WM_QUIT};

pub struct Raccourcis {
    tid: std::sync::Arc<AtomicU32>,
    fil: Mutex<Option<JoinHandle<()>>>,
}

impl Raccourcis {
    pub fn demarrer(ch: Chemins, macro_oui: bool) -> Self {
        let tid = std::sync::Arc::new(AtomicU32::new(0));
        let t = tid.clone();
        let fil = std::thread::spawn(move || vivre(ch, macro_oui, &t));
        Raccourcis { tid, fil: Mutex::new(Some(fil)) }
    }

    pub fn arreter(&self) {
        let fin = Instant::now() + Duration::from_secs(2);
        while self.tid.load(Ordering::Acquire) == 0 && Instant::now() < fin {
            std::thread::sleep(Duration::from_millis(20));
        }
        let tid = self.tid.load(Ordering::Acquire);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(f) = self.fil.lock().ok().and_then(|mut f| f.take()) {
            let fin = Instant::now() + Duration::from_secs(3);
            while !f.is_finished() && Instant::now() < fin {
                std::thread::sleep(Duration::from_millis(20));
            }
            if f.is_finished() {
                let _ = f.join();
            }
        }
    }
}

const ID_MACRO: i32 = 1;

fn vivre(ch: Chemins, macro_oui: bool, tid: &AtomicU32) {
    let mut msg = MSG::default();
    unsafe {
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE); // la file de messages existe avant d'être annoncée
    }
    tid.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    if !macro_oui {
        return;
    }
    let pris = unsafe { RegisterHotKey(None, ID_MACRO, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), MACRO_VK) }.is_err();
    if pris {
        noter(&ch, &format!("macro : raccourci {MACRO_NOM} déjà pris par un autre programme"));
        return;
    }
    // l'OCR prêt avant le premier appui
    let mut jeu = match JeuReel::new() {
        Ok(j) => {
            noter(&ch, &format!("macro prête ({MACRO_NOM})"));
            Some(j)
        }
        Err(ex) => {
            noter(&ch, &format!("macro : {ex}"));
            None
        }
    };
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message != WM_HOTKEY || msg.wParam.0 as i32 != ID_MACRO {
                continue;
            }
            if let Some(j) = jeu.as_mut() {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| macro_peaux::lancer(j)));
                match r {
                    Ok(Ok((_, journal))) => noter(&ch, &journal),
                    Ok(Err(Arret(raison))) => {
                        noter(&ch, &format!("macro arrêtée : {raison}"));
                        j.garder_erreur(&ch);
                    }
                    Err(_) => {
                        noter(&ch, "macro : erreur inattendue");
                        jeu = JeuReel::new().ok();
                    }
                }
            }
            while PeekMessageW(&mut msg, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE).as_bool() {} // appuyés pendant la macro
        }
        let _ = UnregisterHotKey(None, ID_MACRO);
    }
}
