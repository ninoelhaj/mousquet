//! Lire les menus de Baylife à l'écran : capture du coin haut gauche du jeu, OCR de Windows (appelé directement, en
//! français), puis rangement en sous-titre + lignes + ligne choisie + compteur « 8 / 13 ». Seulement l'écran : le jeu
//! n'est jamais lu en mémoire.

use regex::Regex;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode,
    ExifOrientationMode,
};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, ClientToScreen, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::HSTRING;

// géométrie des menus de Baylife en 1080p (elle suit la hauteur de l'écran)
pub const ZONE_L: f64 = 480.0;
pub const ZONE_H: f64 = 560.0;
pub const DROITE: f64 = 370.0;
pub const SOUS_TITRE: (f64, f64) = (110.0, 160.0);
pub const LIGNE_0: f64 = 178.0;
pub const PAS: f64 = 38.0;
pub const LIGNES_MAX: usize = 10;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    pub sous_titre: String,
    pub textes: Vec<String>,
    pub choisie: Option<usize>,
    pub compteur: Option<(usize, usize)>,
}

impl Menu {
    pub fn ouvert(&self) -> bool {
        self.choisie.is_some()
    }
    pub fn ligne(&self) -> &str {
        self.choisie.and_then(|i| self.textes.get(i)).map(String::as_str).unwrap_or("")
    }
}

/// Une image BGRA (haut en premier).
#[derive(Clone)]
pub struct Image {
    pub px: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

#[derive(Clone, Debug)]
pub struct Mot {
    pub ligne: usize,
    pub texte: String,
    pub x: f64,
    pub y: f64,
    pub h: f64,
}

// ============================================================================ textes
/// minuscules, sans accents, espaces simples : « Peau de bête » → « peau de bete ».
pub fn norme(t: &str) -> String {
    let s: String = t
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ÿ' => 'y',
            'ñ' => 'n',
            c => c,
        })
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// norme() sans ponctuation : « Dépose/ » → « depose ».
pub fn lettres(t: &str) -> String {
    let s: String = norme(t).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn communs(a: &[char], b: &[char]) -> usize {
    // Ratcliff/Obershelp (difflib) : le plus long bloc commun, puis à gauche et à droite
    let (mut meilleur, mut ia, mut ib) = (0, 0, 0);
    for i in 0..a.len() {
        for j in 0..b.len() {
            let mut k = 0;
            while i + k < a.len() && j + k < b.len() && a[i + k] == b[j + k] {
                k += 1;
            }
            if k > meilleur {
                (meilleur, ia, ib) = (k, i, j);
            }
        }
    }
    if meilleur == 0 {
        return 0;
    }
    meilleur + communs(&a[..ia], &b[..ib]) + communs(&a[ia + meilleur..], &b[ib + meilleur..])
}

/// Ressemblance de deux textes (0 à 1), comme difflib.SequenceMatcher.ratio().
pub fn ressemblance(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    2.0 * communs(&a, &b) as f64 / (a.len() + b.len()) as f64
}

/// La ligne est ce mot (« Dépose/ » est « deposer », « Déposer le sac » ne l'est pas).
pub fn est(t: &str, mot: &str) -> bool {
    ressemblance(&lettres(t), mot) >= 0.8
}

pub fn commence(t: &str, debut: &str) -> bool {
    let l: String = lettres(t).chars().take(debut.chars().count()).collect();
    ressemblance(&l, debut) >= 0.8
}

/// Un mot du texte ressemble à mot (pour les sous-titres en capitales).
pub fn contient(t: &str, mot: &str) -> bool {
    lettres(t).split(' ').any(|m| ressemblance(m, mot) >= 0.75)
}

/// Les sous-titres des menus sont en capitales ; les lignes, non.
pub fn capitales(t: &str) -> bool {
    let l: Vec<char> = t.chars().filter(|c| c.is_alphabetic()).collect();
    !l.is_empty() && l.iter().filter(|c| c.is_uppercase()).count() as f64 >= 0.7 * l.len() as f64
}

// ============================================================================ capture
/// Coin haut gauche du jeu, là où s'ouvrent les menus : (image, échelle / 1080p).
pub fn capture_jeu(hwnd: HWND) -> Option<(Image, f64)> {
    unsafe {
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r).ok()?;
        let mut p = POINT::default();
        let _ = ClientToScreen(hwnd, &mut p);
        let k = ((r.bottom - r.top) as f64 / 1080.0).max(0.5);
        let (w, h) = ((ZONE_L * k) as i32, (ZONE_H * k) as i32);
        Some((capture_ecran(p.x, p.y, w, h)?, k))
    }
}

pub fn capture_ecran(x: i32, y: i32, w: i32, h: i32) -> Option<Image> {
    unsafe {
        let ecran = GetDC(None);
        let dc = CreateCompatibleDC(Some(ecran));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let hb = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let ancien = SelectObject(dc, hb.into());
        let _ = BitBlt(dc, 0, 0, w, h, Some(ecran), x, y, SRCCOPY);
        let px = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
        SelectObject(dc, ancien);
        let _ = DeleteObject(hb.into());
        let _ = DeleteDC(dc);
        ReleaseDC(None, ecran);
        Some(Image { px, w: w as usize, h: h as usize })
    }
}

// ============================================================================ regard : les pixels seuls, sans OCR
/// Une ligne est claire quand la médiane de ses pixels (le plus faible des trois canaux) dépasse ce seuil : c'est la
/// barre blanche de la ligne choisie (même mesure que `analyser`).
const CLAIR: u8 = 170;

/// La barre blanche de la ligne choisie, trouvée aux pixels seuls (moins d'une milliseconde) : sa place et une
/// empreinte de son texte. Sert à voir qu'une touche a été prise par le jeu (la barre bouge, ou son texte change
/// quand la liste défile) sans attendre l'OCR, et à ne lire à l'OCR que le haut du menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Regard {
    /// Haut et bas de la barre, en pixels de l'image.
    pub haut: usize,
    pub bas: usize,
    /// Milieu de la barre en pixels 1080p : comparable d'une image à l'autre.
    pub y: f64,
    /// Le texte noir sur la barre, en 6 rangs de 64 cases.
    pub empreinte: [u64; 6],
}

fn plus_faible(im: &Image, x: usize, y: usize) -> u8 {
    let j = (y * im.w + x) * 4;
    im.px[j].min(im.px[j + 1]).min(im.px[j + 2])
}

pub fn regard(im: &Image, k: f64) -> Option<Regard> {
    let (x0, x1) = ((44.0 * k) as usize, im.w.min((456.0 * k) as usize));
    let pas = ((6.0 * k) as usize).max(1);
    let y_min = (150.0 * k) as usize; // sous la bannière et le sous-titre
    if x1 <= x0 + 16 || y_min >= im.h {
        return None;
    }
    let mut vals = Vec::with_capacity((x1 - x0) / pas + 1);
    let mut clair = |y: usize| {
        vals.clear();
        vals.extend((x0..x1).step_by(pas).map(|x| plus_faible(im, x, y)));
        let n = vals.len() / 2;
        *vals.select_nth_unstable(n).1 >= CLAIR
    };
    // bandes de rangs clairs ; le texte noir peut couper la barre sur la hauteur des lettres
    let trou = ((14.0 * k) as usize).max(2);
    let mut bandes: Vec<(usize, usize)> = Vec::new();
    for y in y_min..im.h {
        if clair(y) {
            match bandes.last_mut() {
                Some(b) if y <= b.1 + trou => b.1 = y,
                _ => bandes.push((y, y)),
            }
        }
    }
    // une ligne de menu fait 38 px en 1080p : ni un trait, ni un grand ciel blanc
    let bonnes: Vec<(usize, usize)> = bandes
        .into_iter()
        .filter(|(a, b)| (24.0 * k..=50.0 * k).contains(&((b - a + 1) as f64)))
        .collect();
    let [(haut, bas)] = bonnes.as_slice() else { return None };
    let (haut, bas) = (*haut, *bas);
    let mut empreinte = [0u64; 6];
    let h = bas + 1 - haut;
    for (r, e) in empreinte.iter_mut().enumerate() {
        let (ya, yb) = (haut + h * r / 6, haut + h * (r + 1) / 6);
        for c in 0..64 {
            let (xa, xb) = (x0 + (x1 - x0) * c / 64, x0 + (x1 - x0) * (c + 1) / 64);
            let (mut n, mut sombres) = (0usize, 0usize);
            for y in ya..yb {
                for x in xa..xb {
                    let j = (y * im.w + x) * 4;
                    n += 1;
                    sombres += (im.px[j].max(im.px[j + 1]).max(im.px[j + 2]) < 110) as usize;
                }
            }
            if n > 0 && sombres * 8 > n {
                *e |= 1 << c;
            }
        }
    }
    Some(Regard { haut, bas, y: (haut + bas) as f64 / 2.0 / k, empreinte })
}

/// L'écran a changé entre deux regards : menu ouvert ou fermé, barre déplacée, ou autre texte sur la barre.
pub fn differents(a: &Option<Regard>, b: &Option<Regard>) -> bool {
    match (a, b) {
        (None, None) => false,
        (Some(a), Some(b)) => {
            let bits: u32 = a.empreinte.iter().zip(&b.empreinte).map(|(x, y)| (x ^ y).count_ones()).sum();
            (a.y - b.y).abs() > 8.0 || bits > 8
        }
        _ => true,
    }
}

/// Une image en BMP 32 bits, en mémoire.
fn octets_bmp(im: &Image) -> Vec<u8> {
    let n = im.px.len() as u32;
    let mut d = Vec::with_capacity(54 + im.px.len());
    d.extend_from_slice(b"BM");
    d.extend_from_slice(&(54 + n).to_le_bytes());
    d.extend_from_slice(&[0; 4]);
    d.extend_from_slice(&54u32.to_le_bytes());
    d.extend_from_slice(&40u32.to_le_bytes());
    d.extend_from_slice(&(im.w as i32).to_le_bytes());
    d.extend_from_slice(&(-(im.h as i32)).to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&32u16.to_le_bytes());
    d.extend_from_slice(&[0; 4]);
    d.extend_from_slice(&n.to_le_bytes());
    d.extend_from_slice(&[0x13, 0x0B, 0, 0, 0x13, 0x0B, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    d.extend_from_slice(&im.px);
    d
}

/// BMP 32 bits (pour garder l'image d'une erreur de la macro).
pub fn ecrire_bmp(chemin: &std::path::Path, im: &Image) -> std::io::Result<()> {
    std::fs::write(chemin, octets_bmp(im))
}

#[cfg(test)]
pub fn lire_bmp(chemin: &std::path::Path) -> Option<Image> {
    let d = std::fs::read(chemin).ok()?;
    let w = i32::from_le_bytes(d.get(18..22)?.try_into().ok()?);
    let h = i32::from_le_bytes(d.get(22..26)?.try_into().ok()?);
    let debut = u32::from_le_bytes(d.get(10..14)?.try_into().ok()?) as usize;
    Some(Image { px: d.get(debut..)?.to_vec(), w: w as usize, h: h.unsigned_abs() as usize })
}

// ============================================================================ OCR de Windows
pub struct Ocr {
    moteur: OcrEngine,
}

impl Ocr {
    /// L'OCR français de Windows (ou celui de la langue de Windows). À créer dans le fil qui s'en sert.
    pub fn new() -> Result<Self, String> {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let fr = Language::CreateLanguage(&HSTRING::from("fr-FR")).ok().and_then(|l| OcrEngine::TryCreateFromLanguage(&l).ok());
        let moteur = fr.or_else(|| OcrEngine::TryCreateFromUserProfileLanguages().ok()).ok_or("OCR de Windows indisponible")?;
        Ok(Ocr { moteur })
    }

    /// Les mots lus, en pixels de l'image d'origine.
    pub fn lire(&self, im: &Image, ech: f64) -> Result<Vec<Mot>, String> {
        // agrandie par le décodeur d'images de Windows (interpolation Fant) : l'OCR lit nettement mieux qu'avec un
        // simple bilinéaire (mesuré sur 103 captures du jeu)
        let e = |x: windows::core::Error| format!("OCR : {x}");
        let flux = InMemoryRandomAccessStream::new().map_err(e)?;
        let ecrivain = DataWriter::CreateDataWriter(&flux).map_err(e)?;
        ecrivain.WriteBytes(&octets_bmp(im)).map_err(e)?;
        ecrivain.StoreAsync().map_err(e)?.join().map_err(e)?;
        ecrivain.DetachStream().map_err(e)?;
        flux.Seek(0).map_err(e)?;
        let dec = BitmapDecoder::CreateAsync(&flux).map_err(e)?.join().map_err(e)?;
        let (gw, gh) = ((im.w as f64 * ech) as u32, (im.h as f64 * ech) as u32);
        let tr = BitmapTransform::new().map_err(e)?;
        tr.SetScaledWidth(gw).map_err(e)?;
        tr.SetScaledHeight(gh).map_err(e)?;
        tr.SetInterpolationMode(BitmapInterpolationMode::Fant).map_err(e)?;
        let bmp = dec
            .GetSoftwareBitmapTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
                &tr,
                ExifOrientationMode::IgnoreExifOrientation,
                ColorManagementMode::DoNotColorManage,
            )
            .map_err(e)?
            .join()
            .map_err(e)?;
        let res = self.moteur.RecognizeAsync(&bmp).map_err(e)?.join().map_err(e)?;
        let (ex, ey) = (gw as f64 / im.w as f64, gh as f64 / im.h as f64);
        let mut mots = Vec::new();
        for (i, ligne) in res.Lines().map_err(e)?.into_iter().enumerate() {
            for m in ligne.Words().map_err(e)? {
                let r = m.BoundingRect().map_err(e)?;
                mots.push(Mot {
                    ligne: i,
                    texte: m.Text().map_err(e)?.to_string(),
                    x: r.X as f64 / ex,
                    y: r.Y as f64 / ey,
                    h: r.Height as f64 / ey,
                });
            }
        }
        Ok(mots)
    }

    /// Les mots d'une bande de l'image seulement (rangs y0..y1), en pixels de l'image entière : moins de texte à
    /// lire, l'OCR va d'autant plus vite.
    pub fn lire_bande(&self, im: &Image, y0: usize, y1: usize, ech: f64) -> Result<Vec<Mot>, String> {
        let (y0, y1) = (y0.min(im.h), y1.min(im.h));
        if y1 <= y0 + 4 {
            return Ok(Vec::new());
        }
        let bande = Image { px: im.px[y0 * im.w * 4..y1 * im.w * 4].to_vec(), w: im.w, h: y1 - y0 };
        let mut mots = self.lire(&bande, ech)?;
        for m in &mut mots {
            m.y += y0 as f64;
        }
        Ok(mots)
    }
}

// ============================================================================ rangement
/// Range les mots lus en sous-titre + lignes du menu. Le sous-titre commence dans la bande SOUS_TITRE et peut tenir
/// sur deux lignes ; les lignes suivent tous les PAS pixels ; la ligne choisie a un fond blanc sur toute sa largeur.
pub fn analyser(mots: &[Mot], im: &Image, k: f64) -> Menu {
    use std::collections::BTreeMap;
    let mut groupes: BTreeMap<(usize, bool), Vec<&Mot>> = BTreeMap::new();
    for m in mots {
        groupes.entry((m.ligne, m.x >= DROITE * k)).or_default().push(m);
    }
    let mut entrees: Vec<(f64, bool, String)> = groupes
        .iter()
        .map(|((_, droite), ws)| {
            let mut ws = ws.clone();
            ws.sort_by(|a, b| a.x.total_cmp(&b.x));
            let haut = ws.iter().map(|m| m.y).fold(f64::MAX, f64::min);
            let bas = ws.iter().map(|m| m.y + m.h).fold(f64::MIN, f64::max);
            ((haut + bas) / 2.0, *droite, ws.iter().map(|m| m.texte.as_str()).collect::<Vec<_>>().join(" "))
        })
        .collect();
    entrees.sort_by(|a, b| a.0.total_cmp(&b.0));
    let re_compteur = Regex::new(r"^\d+ ?/ ?\d+$").unwrap();
    let (mut sous_titre, mut compteur, mut bas_titre) = (String::new(), None, None::<f64>);
    for (c, droite, texte) in &entrees {
        let dans_bande = SOUS_TITRE.0 * k <= *c && *c <= SOUS_TITRE.1 * k;
        if *droite {
            if dans_bande && re_compteur.is_match(texte) {
                let p: Vec<usize> = texte.split('/').filter_map(|v| v.trim().parse().ok()).collect();
                if p.len() == 2 && 1 <= p[0] && p[0] <= p[1] {
                    compteur = Some((p[0], p[1]));
                }
            }
        } else if capitales(texte)
            && match bas_titre {
                None => dans_bande,
                Some(b) => b - 8.0 * k <= *c && *c <= b + 31.0 * k,
            }
        {
            sous_titre = format!("{sous_titre} {texte}").trim().to_string();
            bas_titre = Some(bas_titre.map_or(*c, |b| b.max(*c)));
        }
    }
    let base = bas_titre.map_or(LIGNE_0 * k, |b| b + 37.0 * k);
    let mut rangs: BTreeMap<usize, String> = BTreeMap::new();
    for (c, droite, texte) in &entrees {
        if *droite || *c < base - 12.0 * k {
            continue;
        }
        let i = ((c - base) / (PAS * k)).round();
        if i >= 0.0 && (c - (base + i * PAS * k)).abs() <= 12.0 * k && (i as usize) < LIGNES_MAX {
            let e = rangs.entry(i as usize).or_default();
            *e = if e.is_empty() { texte.clone() } else { format!("{e} {texte}") };
        }
    }
    // fond blanc au-dessus ET en dessous du texte : une image prise en pleine animation est rejetée
    let mut fonds: Vec<(usize, u8)> = Vec::new();
    for &i in rangs.keys() {
        let yc = base + i as f64 * PAS * k;
        let mut moities = Vec::new();
        for dy in [-10.0, 10.0] {
            let y = (yc + dy * k) as i64;
            let mut vals = Vec::new();
            if y >= 0 && (y as usize) < im.h {
                let mut x = (44.0 * k) as usize;
                while x < im.w.min((456.0 * k) as usize) {
                    let j = (y as usize * im.w + x) * 4;
                    vals.push(im.px[j].min(im.px[j + 1]).min(im.px[j + 2]));
                    x += ((6.0 * k) as usize).max(1);
                }
            }
            vals.sort_unstable();
            moities.push(vals.get(vals.len() / 2).copied().unwrap_or(0));
        }
        fonds.push((i, moities.into_iter().min().unwrap_or(0)));
    }
    fonds.sort_by(|a, b| b.1.cmp(&a.1));
    let choisie = match fonds.as_slice() {
        [(i, v)] if *v >= 170 => Some(*i),
        [(i, v), (_, v2), ..] if *v >= 170 && v.saturating_sub(*v2) >= 50 => Some(*i),
        _ => None,
    };
    let fin = rangs.keys().copied().chain(choisie).max().map_or(0, |m| m + 1);
    let textes: Vec<String> = (0..fin).map(|i| rangs.get(&i).cloned().unwrap_or_default()).collect();
    if let Some((a, b)) = compteur {
        if choisie.is_none_or(|ch| a - 1 < ch) || b < textes.len() {
            compteur = None;
        }
    }
    Menu { sous_titre, textes, choisie, compteur }
}

/// Le compteur « 8 / 13 » relu seul : en noir sur blanc, avec une marge, agrandi. Gardé seulement si deux réglages
/// lisent la même chose (un faux nombre de lignes ferait parcourir la liste pour rien).
pub fn lire_compteur(ocr: &Ocr, im: &Image, m: &Menu, k: f64) -> Option<(usize, usize)> {
    let choisie = m.choisie?;
    let (x0, x1) = ((380.0 * k) as usize, im.w.min((470.0 * k) as usize));
    let (y0, y1) = ((126.0 * k) as usize, im.h.min((156.0 * k) as usize));
    let marge = (24.0 * k) as usize;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let re = Regex::new(r"(\d+) ?/ ?(\d+)").unwrap();
    let lues = m.textes.iter().filter(|t| !t.is_empty()).count();
    let mut vus: Vec<(usize, usize)> = Vec::new();
    for (seuil, ech) in [(130u8, 2.5), (150, 3.0), (110, 2.0)] {
        let (lw, lh) = (x1 - x0 + 2 * marge, y1 - y0 + 2 * marge);
        let mut px = vec![255u8; lw * lh * 4];
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * im.w + x) * 4;
                if im.px[i].max(im.px[i + 1]).max(im.px[i + 2]) > seuil {
                    let j = ((y - y0 + marge) * lw + x - x0 + marge) * 4;
                    px[j..j + 3].copy_from_slice(&[0, 0, 0]);
                }
            }
        }
        let Ok(mut mots) = ocr.lire(&Image { px, w: lw, h: lh }, ech / k) else { continue };
        mots.sort_by(|a, b| a.x.total_cmp(&b.x));
        let texte = mots.iter().map(|m| m.texte.as_str()).collect::<Vec<_>>().join(" ");
        if let Some(c) = re.captures(&texte) {
            let (a, b) = (c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0));
            if 1 <= a && a <= b && a - 1 >= choisie && b >= lues {
                if vus.contains(&(a, b)) {
                    return Some((a, b));
                }
                vus.push((a, b));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macro_peaux::{est_peau, est_venaison};

    /// Une image de menu dessinée : fond sombre, barre blanche à la ligne `i`, traits noirs (le « texte ») dedans.
    fn menu_dessine(i: usize, texte: u64) -> Image {
        let (w, h) = (480, 560);
        let mut px = vec![0u8; w * h * 4];
        for p in px.chunks_mut(4) {
            p.copy_from_slice(&[30, 25, 20, 255]);
        }
        let (haut, bas) = (159 + 38 * i, 159 + 38 * i + 37);
        for y in haut..=bas.min(h - 1) {
            for x in 0..470 {
                let j = (y * w + x) * 4;
                let trait_ = y > haut + 11 && y < bas - 11 && x > 50 && x % 7 < 2 && (texte >> ((x / 7) % 64)) & 1 == 1;
                px[j..j + 3].copy_from_slice(if trait_ { &[10, 10, 10] } else { &[235, 235, 235] });
            }
        }
        Image { px, w, h }
    }

    #[test]
    fn regard_trouve_la_barre_et_voit_les_changements() {
        let (t1, t2) = (0xF0F0_0F0F_3333_CCCC_u64, 0x0F0F_F0F0_CCCC_3333_u64);
        let a = regard(&menu_dessine(2, t1), 1.0).expect("barre");
        assert!((a.y - (159.0 + 76.0 + 18.5)).abs() < 2.0, "{}", a.y);
        let a = Some(a);
        assert!(!differents(&a, &regard(&menu_dessine(2, t1), 1.0)));
        assert!(differents(&a, &regard(&menu_dessine(3, t1), 1.0)), "barre descendue");
        assert!(differents(&a, &regard(&menu_dessine(2, t2), 1.0)), "liste défilée : autre texte");
        assert!(differents(&a, &None), "menu fermé");
        assert!(regard(&Image { px: vec![20; 480 * 560 * 4], w: 480, h: 560 }, 1.0).is_none());
        assert!(regard(&Image { px: vec![240; 480 * 560 * 4], w: 480, h: 560 }, 1.0).is_none(), "tout blanc");
        // 1440p : même menu agrandi
        let m = menu_dessine(4, t1);
        let k = 4.0 / 3.0;
        let (w, h) = ((480.0 * k) as usize, (560.0 * k) as usize);
        let mut px = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (((x as f64 / k) as usize).min(479), ((y as f64 / k) as usize).min(559));
                let (j, sj) = ((y * w + x) * 4, (sy * 480 + sx) * 4);
                px[j..j + 4].copy_from_slice(&m.px[sj..sj + 4]);
            }
        }
        let r = regard(&Image { px, w, h }, k).expect("barre en 1440p");
        assert!((r.y - (159.0 + 152.0 + 18.5)).abs() < 2.0, "{}", r.y);
    }

    /// Lecture de vraies captures du jeu (BMP) : `cargo test captures -- --ignored --nocapture`, dossiers séparés
    /// par « ; » dans MOUSQUET_CAPTURES. Une ligne par image, au même format que l'outil de la version Python.

    /// Vitesse de l'OCR sur de vraies captures : `cargo test --release vitesse_ocr -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn vitesse_ocr() {
        let ocr = Ocr::new().unwrap();
        let dossier = std::env::var("MOUSQUET_CAPTURES").unwrap_or_else(|_| "reglages/macro_erreurs".into());
        let mut f: Vec<_> = std::fs::read_dir(dossier).unwrap().flatten().map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "bmp")).collect();
        f.sort();
        let (mut tout, mut court, mut reg, mut n) = (0.0, 0.0, 0.0, 0);
        for p in f.iter().take(8) {
            let im = lire_bmp(p).unwrap();
            for _ in 0..3 {
                let t = std::time::Instant::now();
                let _ = ocr.lire_bande(&im, 96, im.h, 2.0).unwrap();
                tout += t.elapsed().as_secs_f64();
                let t = std::time::Instant::now();
                let _ = ocr.lire_bande(&im, 96, 250, 2.0).unwrap();
                court += t.elapsed().as_secs_f64();
                let t = std::time::Instant::now();
                let _ = regard(&im, 1.0);
                reg += t.elapsed().as_secs_f64();
                n += 1;
            }
        }
        let t = std::time::Instant::now();
        let mut k = 0;
        while t.elapsed().as_secs_f64() < 1.0 {
            let _ = capture_ecran(0, 0, 480, 560);
            k += 1;
        }
        println!("OCR tout le menu {:.0} ms · haut du menu {:.0} ms · regard {:.2} ms · capture {:.1} ms ({n} essais)",
            1000.0 * tout / n as f64, 1000.0 * court / n as f64, 1000.0 * reg / n as f64, 1000.0 / k as f64);
    }

    #[test]
    #[ignore]
    fn captures() {
        let ocr = Ocr::new().unwrap();
        let mut sortie = String::new();
        for dossier in std::env::var("MOUSQUET_CAPTURES").expect("MOUSQUET_CAPTURES").split(';') {
            let mut f: Vec<_> = std::fs::read_dir(dossier).unwrap().flatten().map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "bmp")).collect();
            f.sort();
            for p in f {
                let im = lire_bmp(&p).unwrap();
                let ech: f64 = std::env::var("MOUSQUET_ECH").ok().and_then(|e| e.parse().ok()).unwrap_or(2.0);
                let m = analyser(&ocr.lire(&im, ech).unwrap(), &im, 1.0);
                let marques: Vec<String> = m.textes.iter().map(|t| {
                    let mut x = String::new();
                    for (ok, c) in [(est_peau(t), 'P'), (est_venaison(t), 'V'), (est(t, "supprimer"), 'S'), (est(t, "oui"), 'O'),
                        (est(t, "deposer"), 'D'), (est(t, "inventaire"), 'I')] {
                        if ok { x.push(c) }
                    }
                    x
                }).collect();
                sortie += &format!("{} | {} | {:?} | {:?} | {}\n", p.file_name().unwrap().to_string_lossy(), m.sous_titre, m.compteur,
                    m.choisie, marques.join(","));
            }
        }
        if let Ok(d) = std::env::var("MOUSQUET_SORTIE") {
            std::fs::write(d, &sortie).unwrap();
        }
        print!("{sortie}");
    }
}
