//! Macro peaux (Ctrl+F7 en jeu) : F2 → Déposer → chaque peau de bête exotique, rare ou non, va au maximum dans ton
//! sac ; puis F5 → Inventaire → chaque venaison → Supprimer → Oui. Si le sac est plein, le reste reste dans
//! l'inventaire.
//!
//! Les menus de Baylife sont des menus RageUI : après chaque appui (flèche, Entrée, gauche/droite, retour), RageUI
//! ignore la même touche pendant 175 ms. Un appui plus rapproché est perdu et le curseur s'arrête une ligne trop tôt :
//! les appuis sont donc espacés de `ECART`. Chaque appui est ensuite vu aux pixels, en quelques millisecondes : la
//! barre blanche de la ligne choisie bouge, ou son texte change quand la liste défile. L'OCR (lent) ne sert qu'à
//! décider : toute la liste quand il faut y chercher une ligne, sinon seulement le haut du menu jusqu'à la ligne
//! choisie. Avant chaque Entrée, la ligne est relue deux fois. Un menu inattendu, ou FiveM qui passe derrière, arrête
//! la macro net ; l'image de ce moment-là et les derniers pas vont dans le journal.

use crate::chemins::Chemins;
use crate::lecture::{self, Image, LIGNES_MAX, Menu, Ocr, Regard, commence, contient, est, lettres};
use crate::systeme;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAP_VIRTUAL_KEY_TYPE, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
};

pub const MACRO_VK: u32 = 0x76; // F7 (avec Ctrl)
pub const MACRO_NOM: &str = "ctrl+F7";
/// Durée d'un appui : au moins une image du jeu même à 25 i/s, loin des 175 ms où RageUI répète une touche tenue.
pub const APPUI: f64 = 0.05;
/// D'un appui au suivant de la même touche : les 175 ms où RageUI l'ignore, plus deux images à 45 i/s (un appui
/// perdu quand même, à moins d'images par seconde, est vu et refait).
pub const ECART: f64 = 0.215;
/// D'un appui à celui d'une autre touche (RageUI a un verrou par touche) : deux images, pour qu'elles restent distinctes.
pub const ECART_AUTRE: f64 = 0.07;
/// Le temps laissé au jeu pour montrer l'effet d'un appui (une ou deux images, sauf gros ralentissement).
const REPONSE: f64 = 0.35;
/// Haut des menus en 1080p : la bannière au-dessus n'est jamais lue.
const HAUT_MENU: f64 = 96.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Touche {
    F2,
    F5,
    Entree,
    Retour,
    Gauche,
    Haut,
    Droite,
    Bas,
}

impl Touche {
    pub fn vk(self) -> u16 {
        match self {
            Touche::F2 => 0x71,
            Touche::F5 => 0x74,
            Touche::Entree => 0x0D,
            Touche::Retour => 0x08,
            Touche::Gauche => 0x25,
            Touche::Haut => 0x26,
            Touche::Droite => 0x27,
            Touche::Bas => 0x28,
        }
    }

    fn nom(self) -> &'static str {
        match self {
            Touche::F2 => "F2",
            Touche::F5 => "F5",
            Touche::Entree => "Entrée",
            Touche::Retour => "Retour",
            Touche::Gauche => "←",
            Touche::Haut => "↑",
            Touche::Droite => "→",
            Touche::Bas => "↓",
        }
    }
}

/// La macro s'arrête : l'écran n'est pas celui attendu, ou FiveM n'est plus devant.
#[derive(Debug)]
pub struct Arret(pub String);

fn arret<T>(raison: &str) -> Result<T, Arret> {
    Err(Arret(raison.to_string()))
}

type R<T> = Result<T, Arret>;

/// Ce que la macro voit et touche du jeu : l'écran et le clavier (un faux jeu dans les tests).
pub trait Jeu {
    /// Attend que Ctrl et F7 soient lâchés, vérifie que le jeu est devant.
    fn pret(&mut self) -> R<()>;
    /// Tout le menu, lu à l'OCR.
    fn lire(&mut self) -> R<Menu>;
    /// Le haut du menu jusqu'à la ligne choisie seulement : sous-titre et ligne choisie, bien plus vite. Les lignes
    /// sous la ligne choisie peuvent manquer.
    fn lire_court(&mut self) -> R<Menu>;
    /// Pixels seuls, sans OCR : la barre de la ligne choisie, None si aucune n'est visible.
    fn regard(&mut self) -> R<Option<Regard>>;
    /// Le compteur « 8 / 13 » de la dernière image, relu seul.
    fn compteur(&mut self) -> Option<(usize, usize)>;
    /// Un appui, brut : la macro les espace elle-même.
    fn appui(&mut self, t: Touche) -> R<()>;
    fn dormir(&mut self, s: f64);
    fn maintenant(&mut self) -> f64;
    /// La macro s'arrête : l'image de ce moment est gardée avant que les menus soient refermés.
    fn figer(&mut self) {}
}

// ============================================================================ le vrai jeu
pub struct JeuReel {
    ocr: Ocr,
    debut: Instant,
    derniere: Option<(Image, Menu, f64)>,
    erreur: Option<(Image, Menu, f64)>,
}

impl JeuReel {
    pub fn new() -> Result<Self, String> {
        Ok(JeuReel { ocr: Ocr::new()?, debut: Instant::now(), derniere: None, erreur: None })
    }

    /// Une erreur : l'image lue au moment de l'arrêt va dans reglages\macro_erreurs (les 8 dernières), ce qui y a été
    /// lu dans le journal, pour voir ce qui a coincé.
    pub fn garder_erreur(&mut self, ch: &Chemins) {
        let Some((im, m, _)) = self.erreur.take().or_else(|| self.derniere.clone()) else { return };
        crate::journal::noter(ch, &format!(
            "  lu : « {} » ligne choisie {:?} compteur {:?} · {:?}",
            m.sous_titre, m.choisie, m.compteur, m.textes
        ));
        let dossier = ch.macro_erreurs();
        if std::fs::create_dir_all(&dossier).is_err() {
            return;
        }
        let nom = crate::journal::horodatage().replace([':', ' '], "").replace('-', "");
        let _ = lecture::ecrire_bmp(&dossier.join(format!("{nom}.bmp")), &im);
        if let Ok(rd) = std::fs::read_dir(&dossier) {
            let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            v.sort();
            for vieux in v.iter().take(v.len().saturating_sub(8)) {
                let _ = std::fs::remove_file(vieux);
            }
        }
    }

    fn capturer(&mut self) -> R<(Image, f64)> {
        let Some(hwnd) = systeme::jeu_devant() else { return arret("FiveM n'est plus devant") };
        lecture::capture_jeu(hwnd).ok_or_else(|| Arret("capture de l'écran ratée".into()))
    }

    /// OCR des rangs HAUT_MENU..bas de l'image (agrandie comme en 1080p × 2), rangé en menu.
    fn ocr_menu(&self, im: &Image, k: f64, bas: usize) -> R<Menu> {
        let mots = self.ocr.lire_bande(im, (HAUT_MENU * k) as usize, bas, 2.0 / k).map_err(Arret)?;
        Ok(lecture::analyser(&mots, im, k))
    }
}

fn envoyer(scan: u16, flags: KEYBD_EVENT_FLAGS) {
    let i = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: scan, dwFlags: flags | KEYEVENTF_SCANCODE, time: 0, dwExtraInfo: 0 },
        },
    };
    unsafe {
        SendInput(&[i], size_of::<INPUT>() as i32);
    }
}

pub fn appuyee(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
}

impl Jeu for JeuReel {
    fn pret(&mut self) -> R<()> {
        // sinon le jeu recevrait Ctrl+F2, Ctrl+Entrée…
        let fin = Instant::now() + Duration::from_secs(3);
        loop {
            if !appuyee(0x11) && !appuyee(MACRO_VK as i32) {
                std::thread::sleep(Duration::from_millis(60));
                break;
            }
            if Instant::now() > fin {
                return arret("Ctrl ou F7 restés appuyés");
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        if systeme::jeu_devant().is_none() {
            return arret("FiveM n'est pas devant");
        }
        Ok(())
    }

    fn lire(&mut self) -> R<Menu> {
        let (im, k) = self.capturer()?;
        let m = self.ocr_menu(&im, k, im.h)?;
        self.derniere = Some((im, m.clone(), k));
        Ok(m)
    }

    fn lire_court(&mut self) -> R<Menu> {
        let (im, k) = self.capturer()?;
        let mut m = Menu::default();
        if let Some(r) = lecture::regard(&im, k) {
            m = self.ocr_menu(&im, k, r.bas + (8.0 * k) as usize)?;
        }
        if !m.ouvert() {
            m = self.ocr_menu(&im, k, im.h)?; // barre pas vue aux pixels : tout le menu
        }
        self.derniere = Some((im, m.clone(), k));
        Ok(m)
    }

    fn regard(&mut self) -> R<Option<Regard>> {
        let (im, k) = self.capturer()?;
        Ok(lecture::regard(&im, k))
    }

    fn compteur(&mut self) -> Option<(usize, usize)> {
        let (im, m, k) = self.derniere.as_ref()?;
        lecture::lire_compteur(&self.ocr, im, m, *k)
    }

    fn appui(&mut self, t: Touche) -> R<()> {
        if systeme::jeu_devant().is_none() {
            return arret("FiveM n'est plus devant");
        }
        let vk = t.vk();
        let scan = unsafe { MapVirtualKeyW(vk as u32, MAP_VIRTUAL_KEY_TYPE(0)) } as u16;
        let ext = if (0x25..=0x28).contains(&vk) { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) }; // flèches
        envoyer(scan, ext);
        std::thread::sleep(Duration::from_secs_f64(APPUI));
        envoyer(scan, ext | KEYEVENTF_KEYUP);
        Ok(())
    }

    fn dormir(&mut self, s: f64) {
        std::thread::sleep(Duration::from_secs_f64(s));
    }

    fn maintenant(&mut self) -> f64 {
        self.debut.elapsed().as_secs_f64()
    }

    fn figer(&mut self) {
        self.erreur = self.derniere.clone();
    }
}

// ============================================================================ le pilote
/// Ce qu'un appui a donné à l'écran.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reponse {
    /// La barre a bougé ou changé : le jeu a pris la touche.
    Vu,
    /// La barre était là et n'a pas bougé : touche perdue (ou sans effet visible).
    Rien,
    /// Pas de barre à l'écran avant l'appui : rien à comparer, l'OCR décidera.
    Aveugle,
}

struct Pilote<'a> {
    j: &'a mut dyn Jeu,
    /// Le moment du dernier appui de chaque touche (verrou de RageUI), et du dernier appui tout court.
    derniers: BTreeMap<u16, f64>,
    dernier: f64,
    /// Les derniers pas (touches et lectures), notés au journal si la macro s'arrête.
    trace: VecDeque<String>,
    /// La dernière lecture, si aucune touche n'a été appuyée depuis : elle compte comme première image de `sur`.
    fraiche: Option<Menu>,
}

fn resume(m: &Menu) -> String {
    if !m.ouvert() {
        return "fermé".into();
    }
    let titre: String = m.sous_titre.chars().take(18).collect();
    let ligne: String = m.ligne().chars().take(24).collect();
    format!("« {titre} » {}:« {ligne} »", m.choisie.unwrap_or(0))
}

impl<'a> Pilote<'a> {
    fn new(j: &'a mut dyn Jeu) -> Self {
        Pilote { j, derniers: BTreeMap::new(), dernier: f64::NEG_INFINITY, trace: VecDeque::new(), fraiche: None }
    }

    fn noter(&mut self, pas: String) {
        if self.trace.len() >= 16 {
            self.trace.pop_front();
        }
        self.trace.push_back(pas);
    }

    // ------------------------------------------------------------------ clavier
    /// Un appui, espacé de ECART du précédent de la même touche. Avec `voir`, attend que l'écran montre la touche
    /// prise.
    fn presser(&mut self, t: Touche, voir: bool) -> R<Reponse> {
        let meme = self.derniers.get(&t.vk()).copied().unwrap_or(f64::NEG_INFINITY);
        let pret = (meme + ECART).max(self.dernier + ECART_AUTRE);
        let reste = pret - self.j.maintenant();
        if reste > 0.0 {
            self.j.dormir(reste);
        }
        let mut avant = None;
        if voir {
            // une image vide (le jeu redessine) : on regarde encore un peu avant d'appuyer à l'aveugle
            for i in 0..3 {
                avant = self.j.regard()?;
                if avant.is_some() || i == 2 {
                    break;
                }
                self.j.dormir(0.015);
            }
        }
        self.dernier = self.j.maintenant();
        self.derniers.insert(t.vk(), self.dernier);
        self.fraiche = None;
        self.j.appui(t)?;
        let r = if avant.is_none() {
            Reponse::Aveugle
        } else {
            // une autre barre : vu tout de suite ; plus de barre du tout : vu seulement sur trois images de suite (une
            // image vide pendant que le jeu redessine le menu ne suffit pas)
            let fin = self.j.maintenant() + REPONSE;
            let mut vides = 0;
            loop {
                let apres = self.j.regard()?;
                vides = if apres.is_none() { vides + 1 } else { 0 };
                if (apres.is_some() && lecture::differents(&avant, &apres)) || vides >= 3 {
                    break Reponse::Vu;
                }
                if self.j.maintenant() > fin {
                    break Reponse::Rien;
                }
                self.j.dormir(0.012);
            }
        };
        let marque = match r {
            Reponse::Vu => "✓",
            Reponse::Rien => "✗",
            Reponse::Aveugle => "",
        };
        self.noter(format!("{}{marque}", t.nom()));
        Ok(r)
    }

    /// n appuis vus un à un. Un appui que le jeu n'a pas pris (il a ramé) est refait, deux fois au plus ; s'il ne
    /// passe toujours pas, false : la macro relit tout et recompte.
    fn presser_n(&mut self, t: Touche, n: usize) -> R<bool> {
        for _ in 0..n {
            let mut essais = 0;
            while self.presser(t, true)? == Reponse::Rien {
                essais += 1;
                if essais > 2 {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn vers(&mut self, i: usize, choisie: usize) -> R<bool> {
        self.presser_n(if i > choisie { Touche::Bas } else { Touche::Haut }, i.abs_diff(choisie))
    }

    // ------------------------------------------------------------------ lectures
    fn lire(&mut self) -> R<Menu> {
        let m = self.j.lire()?;
        self.noter(format!("lu {}", resume(&m)));
        self.fraiche = Some(m.clone());
        Ok(m)
    }

    fn lire_court(&mut self) -> R<Menu> {
        let m = self.j.lire_court()?;
        self.noter(format!("vu {}", resume(&m)));
        self.fraiche = Some(m.clone());
        Ok(m)
    }

    /// Tout le menu, relu quelques fois s'il paraît fermé : à l'ouverture et après une touche, une image peut être vide.
    fn lire_ouvert(&mut self, delai: f64) -> R<Menu> {
        let fin = self.j.maintenant() + delai;
        let mut m = self.lire()?;
        while !m.ouvert() && self.j.maintenant() < fin {
            self.j.dormir(0.04);
            m = self.lire()?;
        }
        Ok(m)
    }

    /// L'écran tel qu'il est : la dernière lecture si aucune touche n'a été appuyée depuis, sinon une lecture courte.
    fn actuel(&mut self) -> R<Menu> {
        match self.fraiche.clone() {
            Some(m) if m.ouvert() => Ok(m),
            _ => self.lire_court_ouvert(0.8),
        }
    }

    /// Pareil, en lecture courte.
    fn lire_court_ouvert(&mut self, delai: f64) -> R<Menu> {
        let fin = self.j.maintenant() + delai;
        let mut m = self.lire_court()?;
        while !m.ouvert() && self.j.maintenant() < fin {
            self.j.dormir(0.04);
            m = self.lire_court()?;
        }
        Ok(m)
    }

    /// Relit (court) jusqu'à ce que la condition soit vraie : le menu, sinon None au bout du délai.
    fn attendre(&mut self, condition: &dyn Fn(&Menu) -> bool, delai: f64) -> R<Option<Menu>> {
        let fin = self.j.maintenant() + delai;
        loop {
            let m = self.lire_court()?;
            if condition(&m) {
                return Ok(Some(m));
            }
            if self.j.maintenant() > fin {
                return Ok(None);
            }
            self.j.dormir(0.03);
        }
    }

    // ------------------------------------------------------------------ navigation
    /// Amène le curseur sur la première ligne qui passe le test, en faisant défiler la liste si elle n'est pas visible.
    /// Rend le menu (curseur dessus, vérifié), ou None si aucune ligne du menu ne passe le test.
    fn aller_a(&mut self, test: &dyn Fn(&str) -> bool) -> R<Option<Menu>> {
        // les menus gardent la position du curseur : souvent, il est déjà sur la bonne ligne
        let m = self.actuel()?;
        if m.ouvert() && test(m.ligne()) {
            return Ok(Some(m));
        }
        let (mut lues, mut essayees) = (BTreeSet::<i64>::new(), BTreeSet::<i64>::new());
        let mut signes: HashSet<(usize, String)> = HashSet::new();
        let mut connues: BTreeMap<usize, String> = BTreeMap::new(); // sans compteur : lignes lues de la page actuelle
        let mut saut = true;
        'recherche: for _ in 0..80 {
            let mut m = self.lire_ouvert(0.8)?;
            if m.ouvert() && m.compteur.is_none() {
                m.compteur = self.j.compteur(); // le compteur est parfois mal lu : relu seul, agrandi
            }
            let Some(c) = m.choisie else { return arret("menu fermé ou curseur introuvable") };
            if m.textes.iter().any(|t| test(t)) {
                for _ in 0..3 {
                    let i = m.textes.iter().position(|t| test(t)).unwrap_or(0);
                    let c = m.choisie.unwrap_or(0);
                    if i == c {
                        return Ok(Some(m));
                    }
                    // chaque appui vu aux pixels ; si l'un n'a rien donné, on relit tout et on recompte. RageUI passe de
                    // la dernière ligne à la première (et l'inverse) : avec le compteur, l'autre bout est parfois plus court
                    let d = i.abs_diff(c);
                    let bouge = match m.compteur {
                        Some((_, n)) if n > d && n - d < d => {
                            self.presser_n(if i > c { Touche::Haut } else { Touche::Bas }, n - d)?
                        }
                        _ => self.vers(i, c)?,
                    };
                    if bouge {
                        if let Some(m2) = self.attendre(&|m: &Menu| m.ouvert() && test(m.ligne()), 0.3)? {
                            return Ok(Some(m2));
                        }
                    }
                    m = self.lire_ouvert(0.8)?;
                    if !m.ouvert() {
                        return arret("menu fermé par le jeu pendant le déplacement");
                    }
                    if test(m.ligne()) {
                        return Ok(Some(m));
                    }
                    if !m.textes.iter().any(|t| test(t)) {
                        continue 'recherche; // la ligne n'a pas été lue cette fois : on la cherche à nouveau
                    }
                }
                return arret("curseur pas sur la bonne ligne");
            }
            // une ligne pas lue (texte pâle) n'est pas vue : le curseur y va, une ligne choisie se lit toujours bien
            let vides: Vec<usize> = if let Some((a, n)) = m.compteur {
                // « 8 / 13 » : on sait quelles lignes ont été lues
                let (haut, n) = (a as i64 - 1 - c as i64, n as i64);
                lues.extend(m.textes.iter().enumerate().filter(|(_, t)| !t.is_empty()).map(|(i, _)| haut + i as i64));
                essayees.insert(haut + c as i64);
                let manque: Vec<i64> = (0..n).filter(|u| !lues.contains(u) && !essayees.contains(u)).collect();
                if manque.is_empty() {
                    return Ok(None);
                }
                let vides: Vec<usize> =
                    (0..(LIGNES_MAX as i64).min(n - haut)).filter(|i| manque.contains(&(haut + i))).map(|i| i as usize).collect();
                if vides.is_empty() {
                    let cur = haut + c as i64;
                    let bas: Vec<i64> = manque.iter().map(|u| (u - cur).rem_euclid(n)).collect();
                    let hautes: Vec<i64> = manque.iter().map(|u| (cur - u).rem_euclid(n)).collect();
                    let (mb, mh) = (*bas.iter().min().unwrap_or(&1), *hautes.iter().min().unwrap_or(&1));
                    let proche = mb.min(mh);
                    let portee = proche + LIGNES_MAX as i64 - 1; // jusqu'à la plus loin des lignes cachées sur la page
                    if mb == proche {
                        self.presser_n(Touche::Bas, *bas.iter().filter(|&&d| d <= portee).max().unwrap_or(&1) as usize)?;
                    } else {
                        self.presser_n(Touche::Haut, *hautes.iter().filter(|&&d| d <= portee).max().unwrap_or(&1) as usize)?;
                    }
                    continue;
                }
                vides
            } else {
                // sans compteur : la liste a fait le tour quand une ligne revient
                if saut {
                    let signe = (c, lettres(m.ligne()));
                    if !m.ligne().is_empty() && signes.contains(&signe) {
                        return Ok(None);
                    }
                    signes.insert(signe);
                    connues.clear();
                }
                for (i, t) in m.textes.iter().enumerate().filter(|(_, t)| !t.is_empty()) {
                    connues.insert(i, t.clone());
                }
                connues.entry(c).or_insert_with(|| "?".into());
                (0..m.textes.len()).filter(|i| !connues.contains_key(i)).collect()
            };
            saut = vides.is_empty();
            if let Some(&i) = vides.iter().min_by_key(|&&i| i.abs_diff(c)) {
                self.vers(i, c)?;
            } else {
                // vers le haut : depuis la première ligne, le curseur saute à la dernière et montre 10 lignes d'un coup
                self.presser_n(Touche::Haut, c + 1)?;
            }
        }
        arret("liste trop longue")
    }

    /// Plus de menu à l'écran : aucune barre aux pixels sur trois images, et l'OCR le confirme.
    fn ferme(&mut self) -> R<bool> {
        let mut barre = false;
        for _ in 0..3 {
            if self.j.regard()?.is_some() {
                barre = true;
                break;
            }
            self.j.dormir(0.03);
        }
        // une barre vue : quatre lectures vides pour dire « fermé » (une image vide pendant une animation ne suffit pas)
        let lectures = if barre { 4 } else { 1 };
        for _ in 0..lectures {
            if self.lire()?.ouvert() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn fermer_menus(&mut self) -> R<()> {
        for _ in 0..6 {
            if self.ferme()? {
                return Ok(());
            }
            self.presser(Touche::Retour, true)?;
        }
        arret("menus pas fermés")
    }

    /// La condition est vraie sur deux images de suite : jamais une décision sur une image de transition.
    /// La lecture déjà faite depuis le dernier appui compte comme première image.
    fn sur(&mut self, condition: &dyn Fn(&Menu) -> bool) -> R<bool> {
        for _ in 0..3 {
            let premiere = self.actuel()?;
            if condition(&premiere) && condition(&self.lire_court_ouvert(0.8)?) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Entrée sur une ligne vérifiée deux fois (encore), puis attend que l'écran change. Si rien ne bouge (touche
    /// perdue par le jeu) et que la ligne est toujours la bonne, une seconde Entrée. Rend le menu changé, ou None.
    fn valider(
        &mut self,
        encore: &dyn Fn(&Menu) -> bool,
        change: &dyn Fn(&Menu) -> bool,
        delai: f64,
        essais: usize,
    ) -> R<Option<Menu>> {
        for _ in 0..essais {
            if !self.sur(encore)? {
                return Ok(None);
            }
            // l'écran n'a pas bougé : touche sans doute perdue, on n'attend pas tout le délai avant de vérifier
            let r = self.presser(Touche::Entree, true)?;
            let delai = if r == Reponse::Rien { delai.min(0.3) } else { delai };
            if let Some(m) = self.attendre(change, delai)? {
                return Ok(Some(m));
            }
            if !encore(&self.lire_court_ouvert(0.8)?) {
                return Ok(None);
            }
        }
        Ok(None)
    }

    /// Ouvre un menu (F2 ou F5), choisit une ligne et attend la liste qu'elle ouvre (mots sans accents).
    fn ouvrir(&mut self, cle: Touche, titre: &str, ligne: &str, liste: &str) -> R<()> {
        let nom = cle.nom();
        self.fermer_menus()?;
        for essai in 0..2 {
            // le jeu a ramé et perdu la touche : un second appui
            self.presser(cle, true)?;
            if self.attendre(&|m: &Menu| m.ouvert() && contient(&m.sous_titre, titre), 1.5)?.is_some() {
                break;
            }
            if essai > 0 || self.lire_ouvert(0.5)?.ouvert() {
                return arret(&format!("menu {nom} pas ouvert"));
            }
        }
        match self.aller_a(&|t: &str| est(t, ligne))? {
            Some(m) if contient(&m.sous_titre, titre) => {}
            _ => return arret(&format!("« {ligne} » introuvable dans le menu {nom}")),
        }
        let encore = |m: &Menu| m.ouvert() && contient(&m.sous_titre, titre) && est(m.ligne(), ligne);
        if self.valider(&encore, &|m: &Menu| m.ouvert() && contient(&m.sous_titre, liste), 1.0, 3)?.is_none() {
            return arret(&format!("liste « {ligne} » pas ouverte"));
        }
        Ok(())
    }

    // ------------------------------------------------------------------ les deux tâches
    /// F2 → Déposer → chaque peau de bête exotique (rare ou non), quantité au maximum. Si le sac ne les prend pas
    /// toutes d'un coup, une par une jusqu'à ce qu'il soit plein ; le reste reste dans l'inventaire.
    fn deposer_peaux(&mut self) -> R<Depot> {
        let (mut vues, mut restes) = (BTreeMap::<bool, usize>::new(), BTreeMap::<bool, usize>::new());
        let mut plein = false;
        let mut fini = false;
        self.ouvrir(Touche::F2, "voulez", "deposer", "capacite")?;
        for _ in 0..8 {
            // plusieurs passes si la quantité n'était pas au maximum
            if !contient(&self.actuel()?.sous_titre, "capacite") {
                self.ouvrir(Touche::F2, "voulez", "deposer", "capacite")?;
            }
            let Some(m) = self.aller_a(&est_peau)? else {
                fini = true;
                break;
            };
            let (rare, q) = (contient(m.ligne(), "rare"), quantite(m.ligne()));
            vues.entry(rare).or_insert(q);
            let meme = move |m: &Menu| m.ouvert() && est_peau(m.ligne()) && contient(m.ligne(), "rare") == rare;
            self.presser(Touche::Gauche, false)?; // de 1, la flèche gauche passe au maximum
            if self.attendre(&meme, 1.0)?.is_none() {
                return arret("ligne de la peau perdue");
            }
            let la = move |m: &Menu| meme(m) && quantite(m.ligne()) == q;
            if self.valider(&la, &|m: &Menu| !la(m), 1.2, 2)?.is_some() {
                continue;
            }
            // le sac ne prend pas tout d'un coup : quantité à 1 (du maximum, la flèche droite repasse à 1), une par une
            self.presser(Touche::Droite, false)?;
            let mut q = q;
            while q > 0 {
                let qq = q;
                let la = move |m: &Menu| meme(m) && quantite(m.ligne()) == qq;
                let Some(m) = self.valider(&la, &|m: &Menu| !la(m), 1.0, 2)? else { break };
                let ligne = m.textes.iter().find(|t| est_peau(t) && contient(t, "rare") == rare).map_or("", |s| s.as_str());
                q = if m.ouvert() && contient(&m.sous_titre, "capacite") { quantite(ligne) } else { q - 1 };
            }
            restes.insert(rare, q);
            plein = true;
            fini = true;
            break;
        }
        if !fini {
            return arret("peaux toujours là après 8 dépôts");
        }
        self.fermer_menus()?;
        let pris = |r: bool| vues.get(&r).copied().unwrap_or(0).saturating_sub(restes.get(&r).copied().unwrap_or(0));
        Ok(Depot { normales: pris(false), rares: pris(true), plein })
    }

    /// F5 → Inventaire → chaque venaison → Supprimer → Oui. Chaque Entrée est précédée d'une vérification : le titre
    /// du menu est bien la venaison et la ligne choisie est bien « Supprimer », puis « Oui ». Rend le nombre vu.
    fn supprimer_venaison(&mut self) -> R<usize> {
        let mut vue: Option<usize> = None;
        self.ouvrir(Touche::F5, "personnel", "inventaire", "capacite")?;
        let sur_supprimer = |m: &Menu| {
            m.ouvert() && contient(&m.sous_titre, "venaison") && !contient(&m.sous_titre, "voulez") && est(m.ligne(), "supprimer")
        };
        let confirmation = |m: &Menu| m.ouvert() && contient(&m.sous_titre, "supprimer") && contient(&m.sous_titre, "venaison");
        let sur_oui = |m: &Menu| confirmation(m) && est(m.ligne(), "oui");
        let sous_menu = |m: &Menu| {
            m.ouvert()
                && contient(&m.sous_titre, "venaison")
                && !contient(&m.sous_titre, "capacite")
                && !contient(&m.sous_titre, "voulez")
        };
        let mut fini = false;
        for _ in 0..40 {
            let m = self.actuel()?;
            if !sous_menu(&m) {
                // revenu au sous-menu après « Oui » : il en reste, on enchaîne
                if !m.ouvert() || !contient(&m.sous_titre, "capacite") {
                    self.ouvrir(Touche::F5, "personnel", "inventaire", "capacite")?;
                }
                let Some(m) = self.aller_a(&est_venaison)? else {
                    fini = true;
                    break;
                };
                vue = vue.or(Some(quantite(m.ligne())));
                self.presser(Touche::Gauche, false)?; // quantité au maximum
                let dans_liste = |m: &Menu| m.ouvert() && contient(&m.sous_titre, "capacite") && est_venaison(m.ligne());
                if self.attendre(&dans_liste, 1.0)?.is_none() {
                    return arret("ligne de la venaison perdue");
                }
                if self.valider(&dans_liste, &sous_menu, 1.0, 3)?.is_none() {
                    return arret("menu de la venaison pas ouvert");
                }
            }
            match self.aller_a(&|t: &str| est(t, "supprimer"))? {
                Some(m) if sur_supprimer(&m) => {}
                _ => return arret("« Supprimer » pas sûr : rien supprimé"),
            }
            if self.valider(&sur_supprimer, &confirmation, 1.5, 3)?.is_none() {
                return arret("pas de confirmation après Supprimer");
            }
            match self.aller_a(&|t: &str| est(t, "oui"))? {
                // le curseur arrive sur « Non »
                Some(m) if sur_oui(&m) => {}
                _ => return arret("« Oui » pas sûr : rien supprimé"),
            }
            if self.valider(&sur_oui, &|m: &Menu| !confirmation(m), 2.5, 3)?.is_none() {
                return arret("venaison pas supprimée");
            }
        }
        if !fini {
            return arret("venaison toujours là après 40 suppressions");
        }
        self.fermer_menus()?;
        Ok(vue.unwrap_or(0))
    }
}

pub fn est_peau(t: &str) -> bool {
    commence(t, "peau de bete exotique")
}

pub fn est_venaison(t: &str) -> bool {
    commence(t, "venaison exotique")
}

/// « Peau de bête exotique (6) » → 6 ; une ligne sans nombre → 1 ; rien → 0.
pub fn quantite(texte: &str) -> usize {
    let re = regex::Regex::new(r"\((\d+)\)").unwrap();
    match re.captures(texte) {
        Some(c) => c[1].parse().unwrap_or(1),
        None => !texte.is_empty() as usize,
    }
}

pub struct Depot {
    pub normales: usize,
    pub rares: usize,
    pub plein: bool,
}

/// Ctrl+F7 : peaux au sac, venaison supprimée. Rend (l'état court, la ligne du journal). Un arrêt rend la raison
/// suivie des derniers pas (touches : ✓ vue à l'écran, ✗ perdue ; lectures).
pub fn lancer(j: &mut dyn Jeu) -> R<(String, String)> {
    j.pret()?;
    let mut p = Pilote::new(j);
    let debut = p.j.maintenant();
    let fait = match p.deposer_peaux() {
        Ok(d) => p.supprimer_venaison().map(|v| (d, v)),
        Err(e) => Err(e),
    };
    let (peaux, venaison) = match fait {
        Ok(v) => v,
        Err(Arret(raison)) => {
            p.j.figer();
            let pas: Vec<String> = p.trace.iter().cloned().collect();
            let _ = p.fermer_menus();
            return Err(Arret(format!("{raison} | derniers pas : {}", pas.join(" › "))));
        }
    };
    let journal = format!(
        "macro : {} peau(x) et {} rare(s) au sac{}, {venaison} venaison(s) supprimée(s) en {:.1} s",
        peaux.normales,
        peaux.rares,
        if peaux.plein { " (sac plein)" } else { "" },
        p.j.maintenant() - debut
    );
    let n = peaux.normales + peaux.rares;
    let sac = match (peaux.plein, n) {
        (true, 0) => "sac plein".to_string(),
        (true, n) => format!("{n} au sac, plein"),
        (false, 0) => String::new(),
        (false, n) => format!("{n} peau{} au sac", if n > 1 { "x" } else { "" }),
    };
    let parts: Vec<&str> = [sac.as_str(), if venaison > 0 { "venaison suppr." } else { "" }].into_iter().filter(|p| !p.is_empty()).collect();
    let etat = if parts.is_empty() { "rien à déposer".to_string() } else { parts.join(" · ") };
    Ok((etat, journal))
}

// ============================================================================ tests : un faux jeu
#[cfg(test)]
mod tests {
    use super::*;

    const VISIBLES: usize = 10;
    const P: &str = "Peau de bête exotique";
    const PR: &str = "Peau de bête exotique rare";
    const V: &str = "Venaison exotique";
    /// RageUI : une touche est ignorée 175 ms après la précédente, plus une image à 30 i/s au pire.
    const VERROU: f64 = 0.175 + 0.034;

    /// Hasard reproductible (pour le bruit de l'OCR et les cas tirés au sort).
    struct Lcg(u64);
    impl Lcg {
        fn f(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn n(&mut self, a: usize, b: usize) -> usize {
            a + (self.f() * (b - a + 1) as f64) as usize
        }
    }

    /// Une empreinte de barre tirée du texte (deux textes différents ne se ressemblent jamais).
    fn empreinte(t: &str) -> [u64; 6] {
        let mut e = [0u64; 6];
        for (k, v) in e.iter_mut().enumerate() {
            let mut h: u64 = 0xcbf29ce484222325 ^ (k as u64).wrapping_mul(0x9E3779B97F4A7C15);
            for b in t.bytes() {
                h = (h ^ b as u64).wrapping_mul(0x100000001b3);
            }
            *v = h;
        }
        e
    }

    /// Des menus façon Baylife (RageUI) : 10 lignes visibles, défilement, curseurs et quantités gardés d'une fois sur
    /// l'autre, retour en haut après la dernière ligne, et chaque touche ignorée pendant 175 ms après la précédente.
    /// Toute Entrée dangereuse (« Déposer le sac », Supprimer sur autre chose que la venaison, dépôt d'autre chose
    /// qu'une peau) est notée dans `danger`.
    struct FauxJeu {
        inv: Vec<(String, usize)>,
        sac: BTreeMap<String, usize>,
        pile: Vec<(String, usize, usize)>, // menu, curseur, première ligne visible
        curseurs: BTreeMap<String, usize>,
        quantites: BTreeMap<String, usize>,
        capacite: Option<usize>,
        ferme_apres_depot: bool,
        avec_compteur: bool,
        bruit: bool,
        rnd: Lcg,
        temps: f64,
        touches: usize,
        verrous: BTreeMap<u16, f64>,
        perdues: usize,
        danger: Option<String>,
        /// temps passé : lectures, lectures courtes, regards, attentes, appuis
        postes: [f64; 5],
    }

    impl FauxJeu {
        fn new(inv: &[(&str, usize)]) -> Self {
            FauxJeu {
                inv: inv.iter().map(|(n, q)| (n.to_string(), *q)).collect(),
                sac: BTreeMap::new(),
                pile: vec![],
                curseurs: BTreeMap::new(),
                quantites: BTreeMap::new(),
                capacite: None,
                ferme_apres_depot: false,
                avec_compteur: true,
                bruit: false,
                rnd: Lcg(inv.len() as u64 * 7 + 1),
                temps: 0.0,
                touches: 0,
                verrous: BTreeMap::new(),
                perdues: 0,
                danger: None,
                postes: [0.0; 5],
            }
        }

        fn lignes(&self, nom: &str) -> Vec<String> {
            let objets = || self.inv.iter().map(|(n, q)| format!("{n} ({q})"));
            match nom {
                "sac" => ["Retirer", "Déposer", "Replier le sac", "Déposer le sac"].map(String::from).to_vec(),
                "perso" => ["Inventaire", "Équipement", "Portefeuille", "Factures", "Porte-clés", "Véhicule", "Documents",
                    "Album photo", "Album de cartes", "Objets", "Paramètres"].map(String::from).to_vec(),
                "deposer" => std::iter::once("Argent".to_string()).chain(objets()).collect(),
                "inventaire" => objets().collect(),
                n if n.starts_with("objet:") => ["Utiliser", "Donner", "Renommer", "Jeter", "Supprimer"].map(String::from).to_vec(),
                _ => ["Non", "Oui"].map(String::from).to_vec(),
            }
        }

        fn sous_titre(&self, nom: &str) -> String {
            match nom {
                "sac" => "QUE VOULEZ-VOUS FAIRE?".into(),
                "perso" => "MENU PERSONNEL".into(),
                "deposer" | "inventaire" => format!("CAPACITÉ: {}/24", self.inv.len()),
                n if n.starts_with("confirme:") => format!("VOULEZ VOUS SUPPRIMER X1 {} ?", n[9..].to_uppercase()),
                n => n.split_once(':').map_or("", |x| x.1).to_uppercase(),
            }
        }

        fn objet(ligne: &str) -> String {
            ligne.rsplit_once(" (").map_or(ligne, |x| x.0).to_string()
        }

        fn q_de(ligne: &str) -> Option<usize> {
            ligne.rsplit_once('(').and_then(|x| x.1.trim_end_matches(')').parse().ok())
        }

        fn ouvrir(&mut self, nom: &str) {
            let n = self.lignes(nom).len();
            let cur = self.curseurs.get(nom).copied().unwrap_or(0).min(n - 1);
            self.pile.push((nom.into(), cur, cur.min(n.saturating_sub(VISIBLES))));
        }

        fn bouger(&mut self, d: i64) {
            let nom = self.pile.last().unwrap().0.clone();
            let n = self.lignes(&nom).len() as i64;
            let m = self.pile.last_mut().unwrap();
            m.1 = (m.1 as i64 + d).rem_euclid(n) as usize;
            if m.1 < m.2 {
                m.2 = m.1;
            }
            if m.1 >= m.2 + VISIBLES {
                m.2 = m.1 + 1 - VISIBLES;
            }
            if d > 0 && m.1 == 0 {
                m.2 = 0;
            }
            if d < 0 && m.1 as i64 == n - 1 {
                m.2 = (n as usize).saturating_sub(VISIBLES);
            }
            self.curseurs.insert(nom, m.1);
        }

        fn une(&mut self, t: Touche) {
            self.touches += 1;
            if std::env::var_os("MQ_TOUCHES").is_some() {
                eprint!("{}{} ", t.nom(), self.pile.last().map_or(String::new(), |p| format!("[{}]", p.0)));
            }
            if self.pile.is_empty() {
                match t {
                    Touche::F2 => self.ouvrir("sac"),
                    Touche::F5 => self.ouvrir("perso"),
                    _ => {}
                }
                return;
            }
            match t {
                Touche::Bas => self.bouger(1),
                Touche::Haut => self.bouger(-1),
                Touche::Retour => {
                    self.pile.pop();
                }
                Touche::Gauche | Touche::Droite => {
                    let (nom, cur, _) = self.pile.last().unwrap().clone();
                    let ligne = self.lignes(&nom)[cur].clone();
                    if let Some(q) = Self::q_de(&ligne) {
                        let o = Self::objet(&ligne);
                        let v = self.quantites.get(&o).copied().unwrap_or(1);
                        let nv = if t == Touche::Gauche {
                            if v <= 1 { q } else { v - 1 }
                        } else if v >= q {
                            1
                        } else {
                            v + 1
                        };
                        self.quantites.insert(o, nv);
                    }
                }
                Touche::Entree => self.entree(),
                _ => {}
            }
        }

        fn entree(&mut self) {
            let (nom, cur, _) = self.pile.last().unwrap().clone();
            let ligne = self.lignes(&nom)[cur].clone();
            let danger = |s: &mut Self, quoi: String| {
                s.danger.get_or_insert(quoi);
            };
            if nom == "sac" {
                if ligne == "Déposer le sac" {
                    danger(self, "Entrée sur « Déposer le sac »".into());
                } else if ligne == "Déposer" {
                    self.ouvrir("deposer");
                }
            } else if nom == "perso" {
                if ligne == "Inventaire" {
                    self.ouvrir("inventaire");
                }
            } else if nom == "deposer" {
                let o = Self::objet(&ligne);
                if !o.starts_with(P) {
                    return danger(self, format!("dépôt de « {ligne} »"));
                }
                let k = self.inv.iter().position(|(n, _)| *n == o).unwrap();
                let q = self.inv[k].1;
                let v = self.quantites.get(&o).copied().unwrap_or(1).min(q);
                if let Some(c) = self.capacite {
                    if v > c {
                        return; // le sac refuse : rien ne bouge
                    }
                    self.capacite = Some(c - v);
                }
                self.inv[k].1 -= v;
                *self.sac.entry(o.clone()).or_insert(0) += v;
                self.quantites.insert(o, 1);
                if self.inv[k].1 == 0 {
                    self.inv.remove(k);
                }
                let n = self.lignes("deposer").len();
                self.pile.last_mut().unwrap().1 = cur.min(n - 1);
                if self.ferme_apres_depot {
                    self.pile.clear();
                }
            } else if nom == "inventaire" {
                self.ouvrir(&format!("objet:{}", Self::objet(&ligne)));
            } else if let Some(o) = nom.strip_prefix("objet:") {
                if ligne == "Supprimer" {
                    self.pile.push((format!("confirme:{o}"), 0, 0)); // le curseur arrive sur « Non »
                } else {
                    danger(self, format!("Entrée sur « {ligne} » de {o}"));
                }
            } else if let Some(o) = nom.strip_prefix("confirme:") {
                let o = o.to_string();
                self.pile.pop();
                if ligne == "Non" {
                    return;
                }
                if !o.starts_with("Venaison") {
                    return danger(self, format!("Supprimer sur « {o} »"));
                }
                let k = self.inv.iter().position(|(n, _)| *n == o).unwrap();
                self.inv[k].1 -= 1; // comme Baylife : une seule à chaque fois (« X1 »)
                if self.inv[k].1 > 0 {
                    return; // il en reste : on reste dans le sous-menu, sur Supprimer
                }
                self.inv.remove(k);
                self.pile.pop();
                let n = self.lignes("inventaire").len();
                if let Some(inv) = self.pile.last_mut() {
                    inv.1 = inv.1.min(n.saturating_sub(1));
                }
            }
        }

        fn lecture(&mut self, cout: f64, courte: bool) -> R<Menu> {
            self.temps += cout;
            self.postes[courte as usize] += cout;
            if let Some(d) = &self.danger {
                return Err(Arret(format!("DANGER {d}")));
            }
            let Some((nom, cur, haut)) = self.pile.last().cloned() else { return Ok(Menu::default()) };
            if self.bruit && self.rnd.f() < 0.12 {
                return Ok(Menu::default()); // image vide pendant une animation du menu
            }
            let lignes = self.lignes(&nom);
            let mut vis: Vec<String> = lignes.iter().skip(haut).take(VISIBLES).cloned().collect();
            if nom.starts_with("objet:") {
                vis.push("Attention, cette action supprime".into()); // texte sous la liste
                vis.push("définitivement les objets.".into());
            }
            if self.bruit {
                // l'OCR : lignes pâles pas lues, lettres perdues (jamais la ligne choisie)
                for i in 0..vis.len() {
                    if i == cur - haut {
                        continue;
                    }
                    let x = self.rnd.f();
                    if x < 0.15 {
                        vis[i].clear();
                    } else if x < 0.3 && vis[i].chars().count() > 4 {
                        let mut c: Vec<char> = vis[i].chars().collect();
                        let j = (self.rnd.f() * c.len() as f64) as usize;
                        c.remove(j.min(c.len() - 1));
                        vis[i] = c.into_iter().collect();
                    }
                }
                while vis.last().is_some_and(|l| l.is_empty()) && vis.len() - 1 != cur - haut {
                    vis.pop();
                }
            }
            if courte {
                vis.truncate(cur - haut + 1); // lu seulement jusqu'à la ligne choisie
            }
            Ok(Menu {
                sous_titre: self.sous_titre(&nom),
                textes: vis,
                choisie: Some(cur - haut),
                compteur: self.avec_compteur.then_some((cur + 1, lignes.len())),
            })
        }
    }

    impl Jeu for FauxJeu {
        fn pret(&mut self) -> R<()> {
            Ok(())
        }
        // coûts mesurés sur une vraie capture (capture 6 ms ; OCR de tout le menu ~15-60 ms, du haut ~5-20 ms)
        fn lire(&mut self) -> R<Menu> {
            self.lecture(0.06, false)
        }
        fn lire_court(&mut self) -> R<Menu> {
            self.lecture(0.025, true)
        }
        fn regard(&mut self) -> R<Option<Regard>> {
            self.temps += 0.007;
            self.postes[2] += 0.007;
            if let Some(d) = &self.danger {
                return Err(Arret(format!("DANGER {d}")));
            }
            let Some((nom, cur, haut)) = self.pile.last().cloned() else { return Ok(None) };
            if self.bruit && self.rnd.f() < 0.05 {
                return Ok(None);
            }
            let ligne = self.lignes(&nom)[cur].clone();
            let y = 178.0 + (cur - haut) as f64 * 38.0;
            Ok(Some(Regard { haut: 0, bas: 0, y, empreinte: empreinte(&format!("{nom}|{ligne}")) }))
        }
        fn compteur(&mut self) -> Option<(usize, usize)> {
            None
        }
        fn appui(&mut self, t: Touche) -> R<()> {
            self.temps += APPUI;
            self.postes[4] += APPUI;
            let pris = self.temps - self.verrous.get(&t.vk()).copied().unwrap_or(f64::MIN) >= VERROU;
            let perdue = self.bruit && self.rnd.f() < 0.04; // le jeu rame
            if pris && !perdue {
                self.verrous.insert(t.vk(), self.temps);
                self.une(t);
            } else {
                self.perdues += 1;
            }
            Ok(())
        }
        fn dormir(&mut self, s: f64) {
            self.temps += s;
            self.postes[3] += s;
        }
        fn maintenant(&mut self) -> f64 {
            self.temps
        }
    }

    fn nourriture() -> Vec<(&'static str, usize)> {
        vec![
            ("1 - Atom Burger", 1), ("2 - Hamburger au thon", 1), ("3 - Noodles au poulet", 1), ("4 - Quesadilla", 1),
            ("5 - Carson Wrap", 1), ("7 - Salade de gibier", 1), ("9 - Punk Soda", 5), ("Carte de crédit", 1),
            ("Radio", 1), ("Téléphone", 1),
        ]
    }

    fn avec(mut base: Vec<(&'static str, usize)>, plus: &[(&'static str, usize)]) -> Vec<(&'static str, usize)> {
        base.extend_from_slice(plus);
        base
    }

    fn reste(j: &FauxJeu, n: &str) -> usize {
        j.inv.iter().find(|(o, _)| o == n).map_or(0, |x| x.1)
    }

    fn verifier(j: &mut FauxJeu, sac: &[(&str, usize)], inv: &[(&str, usize)], etat: Option<&str>) -> f64 {
        let r = lancer(j);
        assert!(j.danger.is_none(), "danger : {:?}", j.danger);
        let (e, _) = r.unwrap_or_else(|ex| panic!("arrêt : {}", ex.0));
        if let Some(etat) = etat {
            assert_eq!(e, etat);
        }
        let attendu: BTreeMap<String, usize> = sac.iter().map(|(n, q)| (n.to_string(), *q)).collect();
        assert_eq!(j.sac, attendu, "sac");
        for o in [P, PR, V] {
            assert_eq!(reste(j, o), inv.iter().find(|x| x.0 == o).map_or(0, |x| x.1), "reste de {o}");
        }
        assert!(j.pile.is_empty(), "menus restés ouverts : {:?}", j.pile);
        // les appuis sont assez espacés : RageUI n'en perd aucun
        assert_eq!(j.perdues, 0, "touches perdues par le verrou de RageUI");
        j.temps
    }

    #[test]
    fn a_normal_avec_defilement() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 6), (PR, 2), (V, 8), ("Couteau", 1)]));
        let t = verifier(&mut j, &[(P, 6), (PR, 2)], &[], Some("8 peaux au sac · venaison suppr."));
        let p = j.postes;
        println!("durée simulée : {t:.1} s, {} touches · lectures {:.1} s, courtes {:.1} s, regards {:.1} s, attentes {:.1} s, appuis {:.1} s",
            j.touches, p[0], p[1], p[2], p[3], p[4]);
    }

    /// Une chasse ordinaire : deux peaux, une rare, deux venaisons, curseurs restés ailleurs.
    #[test]
    fn o_chasse_ordinaire() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 2), (PR, 1), (V, 2)]));
        j.curseurs.insert("deposer".into(), 4);
        j.curseurs.insert("inventaire".into(), 2);
        let t = verifier(&mut j, &[(P, 2), (PR, 1)], &[], Some("3 peaux au sac · venaison suppr."));
        let p = j.postes;
        println!("chasse ordinaire : {t:.1} s simulées, {} touches · lectures {:.1} s, courtes {:.1} s, regards {:.1} s, attentes {:.1} s, appuis {:.1} s",
            j.touches, p[0], p[1], p[2], p[3], p[4]);
        assert!(t < 10.0, "{t:.1} s");
    }

    #[test]
    fn b_curseurs_gardes_n_importe_ou() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 3), (V, 2)]));
        for (m, c) in [("sac", 3), ("perso", 9), ("deposer", 7), ("inventaire", 8), ("objet:Radio", 4), ("objet:Venaison exotique", 4)] {
            j.curseurs.insert(m.into(), c);
        }
        verifier(&mut j, &[(P, 3)], &[], Some("3 peaux au sac · venaison suppr."));
    }

    #[test]
    fn c_quantite_gardee() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 7), (PR, 1), (V, 5)]));
        j.quantites.insert(P.into(), 3);
        j.quantites.insert(V.into(), 2);
        verifier(&mut j, &[(P, 7), (PR, 1)], &[], Some("8 peaux au sac · venaison suppr."));
    }

    #[test]
    fn d_rien_a_faire() {
        let mut j = FauxJeu::new(&nourriture());
        verifier(&mut j, &[], &[], Some("rien à déposer"));
    }

    #[test]
    fn e_venaison_seule() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(V, 4)]));
        verifier(&mut j, &[], &[], Some("venaison suppr."));
    }

    #[test]
    fn f_rares_en_tete() {
        let mut inv = vec![(PR, 3)];
        inv.extend(nourriture());
        let mut j = FauxJeu::new(&inv);
        verifier(&mut j, &[(PR, 3)], &[], Some("3 peaux au sac"));
    }

    #[test]
    fn g_sac_plein_le_reste_reste_dans_l_inventaire() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 2), (V, 1)]));
        j.capacite = Some(0);
        verifier(&mut j, &[], &[(P, 2)], Some("sac plein · venaison suppr."));
    }

    #[test]
    fn k_sac_presque_plein() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 8), (PR, 2), (V, 1)]));
        j.capacite = Some(5);
        verifier(&mut j, &[(P, 5)], &[(P, 3), (PR, 2)], Some("5 au sac, plein · venaison suppr."));
    }

    #[test]
    fn l_quantite_gardee_sac_de_4() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(PR, 6), (V, 2)]));
        j.capacite = Some(4);
        j.quantites.insert(PR.into(), 3);
        verifier(&mut j, &[(PR, 4)], &[(PR, 2)], None);
    }

    #[test]
    fn h_menu_ferme_apres_chaque_depot() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 2), (PR, 2), (V, 1)]));
        j.ferme_apres_depot = true;
        verifier(&mut j, &[(P, 2), (PR, 2)], &[], Some("4 peaux au sac · venaison suppr."));
    }

    fn longue() -> Vec<(&'static str, usize)> {
        const OBJETS: [&str; 15] = ["Objet 00", "Objet 01", "Objet 02", "Objet 03", "Objet 04", "Objet 05", "Objet 06",
            "Objet 07", "Objet 08", "Objet 09", "Objet 10", "Objet 11", "Objet 12", "Objet 13", "Objet 14"];
        let mut inv = nourriture();
        inv.extend(OBJETS.iter().map(|o| (*o, 1)));
        inv.extend([(V, 3), (PR, 1), (P, 9)]);
        inv
    }

    #[test]
    fn i_longue_liste_avec_et_sans_compteur() {
        for compteur in [true, false] {
            let mut j = FauxJeu::new(&longue());
            j.avec_compteur = compteur;
            j.curseurs.insert("deposer".into(), 12);
            j.curseurs.insert("inventaire".into(), 20);
            let t = verifier(&mut j, &[(P, 9), (PR, 1)], &[], Some("10 peaux au sac · venaison suppr."));
            println!("longue liste (compteur {compteur}) : {t:.1} s simulées");
        }
    }

    #[test]
    fn m_beaucoup_de_venaison() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 2), (V, 25)]));
        verifier(&mut j, &[(P, 2)], &[], Some("2 peaux au sac · venaison suppr."));
    }

    #[test]
    fn j_hasard_200_cas_avec_ocr_bruite_et_touches_perdues() {
        let mut rnd = Lcg(3);
        let noms: Vec<String> = (0..15).map(|i| format!("Objet {i:02}")).collect();
        let mut temps: Vec<(f64, usize, [f64; 5], usize, usize)> = Vec::new();
        for k in 0..200 {
            let mut objets: Vec<(&str, usize)> = nourriture();
            objets.extend(noms.iter().take(rnd.n(0, 14)).map(|n| (n.as_str(), 1)));
            for (o, max) in [(P, 9), (PR, 4), (V, 9)] {
                let q = rnd.n(0, max);
                if q > 0 {
                    let at = rnd.n(0, objets.len());
                    objets.insert(at, (o, q));
                }
            }
            let sac: Vec<(&str, usize)> = objets.iter().filter(|(o, _)| *o == P || *o == PR).copied().collect();
            let mut j = FauxJeu::new(&objets);
            for m in ["sac", "perso", "deposer", "inventaire"] {
                j.curseurs.insert(m.into(), rnd.n(0, 30));
            }
            j.curseurs.insert(format!("objet:{V}"), rnd.n(0, 4));
            for o in [P, PR, V] {
                j.quantites.insert(o.into(), rnd.n(1, 5));
            }
            j.avec_compteur = k % 2 == 0;
            j.bruit = k % 4 < 2;
            let r = lancer(&mut j);
            assert!(j.danger.is_none(), "cas {k} : danger {:?}", j.danger);
            assert!(r.is_ok(), "cas {k} : arrêt {:?}", r.err());
            let attendu: BTreeMap<String, usize> = sac.iter().map(|(n, q)| (n.to_string(), *q)).collect();
            assert_eq!(j.sac, attendu, "cas {k}");
            assert!([P, PR, V].iter().all(|o| reste(&j, o) == 0), "cas {k} : il reste {:?}", j.inv);
            assert!(j.pile.is_empty(), "cas {k} : menus restés ouverts {:?} · {:?}", j.pile, r);
            if !j.bruit {
                assert_eq!(j.perdues, 0, "cas {k} : touches perdues par le verrou de RageUI");
            }
            temps.push((j.temps, k, j.postes, j.touches, j.perdues));
        }
        temps.sort_by(|a, b| a.0.total_cmp(&b.0));
        println!("200 cas : médiane {:.1} s, pire {:.1} s (simulées)", temps[100].0, temps[199].0);
        for t in &temps[195..] {
            println!("  cas {} : {:.1} s · postes {:?} · {} touches, {} perdues", t.1, t.0, t.2.map(|x| (x * 10.0).round() / 10.0), t.3, t.4);
        }
    }

    /// L'ancienne cadence (un appui toutes les 130 ms) perd des touches dans RageUI : c'est elle qui arrêtait la
    /// macro sur « curseur pas sur la bonne ligne ».
    #[test]
    fn n_ancienne_cadence_perdait_des_touches() {
        let mut j = FauxJeu::new(&avec(nourriture(), &[(P, 1)]));
        j.une(Touche::F2);
        for _ in 0..3 {
            j.appui(Touche::Bas).unwrap();
            j.temps += 0.13 - APPUI;
        }
        assert!(j.perdues > 0);
    }

    #[test]
    fn quantites_et_reconnaissance() {
        assert_eq!(quantite("Peau de bête exotique (6)"), 6);
        assert_eq!(quantite("Déposer"), 1);
        assert_eq!(quantite(""), 0);
        assert!(est_peau("Pea de bête exotique rare (2)"));
        assert!(!est_peau("Peau de cerf (2)"));
        assert!(est_venaison("Ve aison exotique (8)"));
        assert!(est("Dépose/", "deposer"));
        assert!(!est("Déposer le sac", "deposer"));
        assert!(contient("CAPACITÉ: 8/48", "capacite"));
    }
}
