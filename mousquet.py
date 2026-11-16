"""mousquet : mode chasse pour Cayo Perico (FiveM).
Le temps de la chasse, FiveM tourne « tout neuf » : tes mods et ton pack graphique sont mis de côté, les graphismes
au plus bas, et chaque mod de chasse (lowbush, stamina…) seulement si tu dis oui. Quand FiveM se ferme, tout revient
exactement à sa place.
Lancé par mousquet.bat. q ou une relance ferment mousquet : un gardien discret remet alors tout à la fermeture de FiveM."""
import ctypes
import ctypes.wintypes as wt
import math
import msvcrt
import os
import random
import sys
import time
import traceback

from datetime import datetime

ICI = os.path.dirname(os.path.abspath(__file__))
JOURNAL = os.path.join(ICI, "mousquet.log")
EVENEMENT = "Local\\MousquetStop"
FIVEM_EXE = os.path.join(os.environ.get("LOCALAPPDATA", ""), "FiveM", "FiveM.exe")     # ton FiveM
FIVEM_APP = os.path.join(os.path.dirname(FIVEM_EXE), "FiveM.app")
DE_COTE = os.path.join(os.path.dirname(FIVEM_EXE), "mis_de_cote_par_mousquet")     # tes mods pendant la chasse
REGLAGES_JEU = os.path.join(os.environ.get("APPDATA", ""), "CitizenFX", "gta5_settings.xml")
DOSSIER_REGLAGES = os.path.join(ICI, "reglages")
MODS_CHASSE = os.path.join(ICI, "mods_chasse")    # les .rpf mis ici vont dans FiveM le temps de la chasse
ATTENTE_FERMETURE = 10     # s après la fermeture de FiveM avant de tout remettre (il se relance parfois tout seul)
ATTENTE_LANCEMENT = 90     # s : si FiveM n'apparaît jamais, tout est remis

# ============================ WINDOWS =============================
k32 = ctypes.WinDLL("kernel32", use_last_error=True)


class COORD(ctypes.Structure):
    _fields_ = [("X", ctypes.c_short), ("Y", ctypes.c_short)]


class SMALL_RECT(ctypes.Structure):
    _fields_ = [("Left", ctypes.c_short), ("Top", ctypes.c_short), ("Right", ctypes.c_short), ("Bottom", ctypes.c_short)]


class CSBIEX(ctypes.Structure):
    _fields_ = [("cbSize", wt.ULONG), ("dwSize", COORD), ("dwCursorPosition", COORD), ("wAttributes", wt.WORD),
                ("srWindow", SMALL_RECT), ("dwMaximumWindowSize", COORD), ("wPopupAttributes", wt.WORD),
                ("bFullscreenSupported", wt.BOOL), ("ColorTable", wt.DWORD * 16)]


k32.GetStdHandle.restype = wt.HANDLE
k32.GetConsoleMode.argtypes = [wt.HANDLE, ctypes.POINTER(wt.DWORD)]
k32.SetConsoleMode.argtypes = [wt.HANDLE, wt.DWORD]
k32.GetConsoleScreenBufferInfoEx.argtypes = [wt.HANDLE, ctypes.POINTER(CSBIEX)]
k32.SetConsoleScreenBufferInfoEx.argtypes = [wt.HANDLE, ctypes.POINTER(CSBIEX)]
k32.OpenEventW.argtypes = [wt.DWORD, wt.BOOL, wt.LPCWSTR]
k32.OpenEventW.restype = wt.HANDLE
k32.CreateEventW.argtypes = [wt.LPVOID, wt.BOOL, wt.BOOL, wt.LPCWSTR]
k32.CreateEventW.restype = wt.HANDLE
k32.SetEvent.argtypes = [wt.HANDLE]
k32.WaitForSingleObject.argtypes = [wt.HANDLE, wt.DWORD]
k32.CloseHandle.argtypes = [wt.HANDLE]
k32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
k32.OpenProcess.restype = wt.HANDLE


def noter(message):
    try:
        with open(JOURNAL, "a", encoding="utf-8") as f:
            f.write(f"{datetime.now():%Y-%m-%d %H:%M:%S}  {message}\n")
    except OSError:
        pass


# ============================ MODE CHASSE ==========================
# Pendant la chasse, mousquet met de côté les mods et le pack graphique de ton FiveM : ils sont déplacés (même
# disque : instantané, 0 Go en plus) dans FiveM\mis_de_cote_par_mousquet. Il met les mods de chasse auxquels tu dis
# oui et les réglages graphiques les plus bas. Quand FiveM se ferme, tout revient exactement à sa place. Une
# liste de chaque élément déplacé est écrite AVANT chaque déplacement : même après une panne, tout peut revenir.
class PROCESSENTRY32W(ctypes.Structure):
    _fields_ = [("dwSize", wt.DWORD), ("cntUsage", wt.DWORD), ("th32ProcessID", wt.DWORD),
                ("th32DefaultHeapID", ctypes.c_void_p), ("th32ModuleID", wt.DWORD), ("cntThreads", wt.DWORD),
                ("th32ParentProcessID", wt.DWORD), ("pcPriClassBase", ctypes.c_long), ("dwFlags", wt.DWORD),
                ("szExeFile", ctypes.c_wchar * 260)]


k32.CreateToolhelp32Snapshot.argtypes = [wt.DWORD, wt.DWORD]
k32.CreateToolhelp32Snapshot.restype = wt.HANDLE
k32.Process32FirstW.argtypes = [wt.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]
k32.Process32NextW.argtypes = [wt.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]


def processus_fivem():
    """Noms des programmes FiveM qui tournent (le jeu, son navigateur, etc.)."""
    noms = []
    snap = k32.CreateToolhelp32Snapshot(0x2, 0)
    if not snap or snap == wt.HANDLE(-1).value:
        return noms
    try:
        pe = PROCESSENTRY32W()
        pe.dwSize = ctypes.sizeof(PROCESSENTRY32W)
        ok = k32.Process32FirstW(snap, ctypes.byref(pe))
        while ok:
            if pe.szExeFile.lower().startswith("fivem"):
                noms.append(pe.szExeFile)
            ok = k32.Process32NextW(snap, ctypes.byref(pe))
    finally:
        k32.CloseHandle(snap)
    return noms


def fivem_ouvert():
    return bool(processus_fivem())


# réglages graphiques de chasse (les plus bas du jeu : « Normal »), appliqués sur une copie de tes réglages : la
# résolution, le mode fenêtré, la fréquence et la densité de population ne bougent pas
REGLAGES_BAS = {
    "Tessellation": "0", "LodScale": "0.000000", "PedLodBias": "0.000000", "VehicleLodBias": "0.000000",
    "ShadowQuality": "1", "ReflectionQuality": "0", "ReflectionMSAA": "0", "SSAO": "0", "AnisotropicFiltering": "0",
    "MSAA": "0", "TextureQuality": "0", "ParticleQuality": "0", "WaterQuality": "0", "GrassQuality": "0",
    "ShaderQuality": "0", "Shadow_SoftShadows": "0", "UltraShadows_Enabled": "false",
    "Shadow_ParticleShadows": "false", "Shadow_LongShadows": "false", "Reflection_MipBlur": "false",
    "FXAA_Enabled": "false", "TXAA_Enabled": "false", "Lighting_FogVolumes": "false", "Shader_SSA": "false",
    "PostFX": "0", "DoF": "false", "MaxLodScale": "0.000000", "MotionBlurStrength": "0.000000",
}


def reglages_bas(xml):
    import re
    for cle, val in REGLAGES_BAS.items():
        xml = re.sub(rf'(<{cle} value=")[^"]*(")', rf"\g<1>{val}\g<2>", xml, count=1)
    return xml


def chemins_reglages():
    os.makedirs(DOSSIER_REGLAGES, exist_ok=True)
    return (os.path.join(DOSSIER_REGLAGES, "normal.xml"), os.path.join(DOSSIER_REGLAGES, "chasse.xml"),
            os.path.join(DOSSIER_REGLAGES, "EN_CHASSE"))


def mettre_reglages_chasse():
    """Garde tes réglages (reglages\\normal.xml) puis met ceux de chasse. Sans effet s'ils sont déjà en place."""
    import shutil
    normal, chasse, marque = chemins_reglages()
    if os.path.exists(marque):
        return
    shutil.copy2(REGLAGES_JEU, normal)
    if open(normal, "rb").read() != open(REGLAGES_JEU, "rb").read():
        raise OSError("copie de tes réglages ratée")
    with open(marque, "w", encoding="utf-8") as f:
        f.write(datetime.now().isoformat())
    if not os.path.exists(chasse):
        with open(chasse, "w", encoding="utf-8", newline="") as f:
            f.write(reglages_bas(open(normal, encoding="utf-8").read()))
    shutil.copy2(chasse, REGLAGES_JEU)
    noter("réglages de chasse mis (les tiens sont gardés dans reglages\\normal.xml)")


def remettre_reglages():
    """Remet tes réglages si ceux de chasse sont en place (FiveM fermé). True si rien à faire ou si c'est fait."""
    import shutil
    normal, chasse, marque = chemins_reglages()
    if not os.path.exists(marque):
        return True
    if fivem_ouvert():
        return False
    shutil.copy2(REGLAGES_JEU, chasse)       # tes retouches faites en chasse sont gardées pour la prochaine fois
    shutil.copy2(normal, REGLAGES_JEU)
    os.remove(marque)
    noter("réglages normaux remis")
    return True


# mods de chasse connus : nom affiché et ce qu'ils changent, pour l'avertissement (chaque mod est demandé à chaque chasse)
AVANTAGES = {"lowbush": ("lowbush", "enlève des buissons"), "stamina": ("stamina", "endurance infinie")}


def presentation(mod):
    """(nom affiché, ce que le mod change ou None) pour un .rpf de mods_chasse."""
    m = mod.lower()
    for cle, (nom, effet) in AVANTAGES.items():
        if cle in m:
            return nom, effet
    return os.path.splitext(mod)[0][:14], None


def mods_de_chasse(refuses=()):
    """Les .rpf de mousquet\\mods_chasse à mettre pour cette chasse (jamais de .dll ni de .asi : des programmes)."""
    refuses = {f.lower() for f in refuses}
    try:
        return sorted(f for f in os.listdir(MODS_CHASSE) if f.lower().endswith(".rpf")
                      and os.path.isfile(os.path.join(MODS_CHASSE, f)) and f.lower() not in refuses)
    except OSError:
        return []


def _liste():
    return os.path.join(DE_COTE, "liste.json")


def en_chasse():
    """True si tes mods et ton pack sont mis de côté (chasse en cours ou pas encore remise)."""
    return os.path.exists(_liste())


def _ecrire_liste(liste):
    import json
    tmp = _liste() + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(liste, f, ensure_ascii=False, indent=1)
    os.replace(tmp, _liste())


def mettre_de_cote(mods):
    """FiveM fermé : déplace tout ce qui est dans FiveM.app\\mods et FiveM.app\\plugins vers DE_COTE, puis copie
    les mods de chasse dans FiveM.app\\mods."""
    import shutil
    if en_chasse() or fivem_ouvert():
        return 0
    os.makedirs(DE_COTE, exist_ok=True)
    liste = {"debut": datetime.now().isoformat(), "deplaces": [], "ajoutes": []}
    _ecrire_liste(liste)
    for dossier in ("mods", "plugins"):
        d = os.path.join(FIVEM_APP, dossier)
        if not os.path.isdir(d):
            continue
        os.makedirs(os.path.join(DE_COTE, dossier), exist_ok=True)
        for nom in sorted(os.listdir(d)):
            rel = os.path.join(dossier, nom)
            liste["deplaces"].append(rel)
            _ecrire_liste(liste)                   # noté avant de bouger : une panne ne perd rien
            os.rename(os.path.join(FIVEM_APP, rel), os.path.join(DE_COTE, rel))
    os.makedirs(os.path.join(FIVEM_APP, "mods"), exist_ok=True)
    for f in mods:
        rel = os.path.join("mods", f)
        liste["ajoutes"].append(rel)
        _ecrire_liste(liste)
        shutil.copy2(os.path.join(MODS_CHASSE, f), os.path.join(FIVEM_APP, rel))
    noter(f"chasse : {len(liste['deplaces'])} élément(s) de tes mods et de ton pack mis de côté · mods de chasse : "
          f"{', '.join(mods) or 'aucun'}")
    return len(liste["deplaces"])


def mods_ajoutes():
    """Les mods de chasse mis pour la chasse en cours."""
    import json
    try:
        return [os.path.basename(r) for r in json.load(open(_liste(), encoding="utf-8")).get("ajoutes", [])]
    except (OSError, ValueError):
        return []


def remettre_de_cote():
    """FiveM fermé : enlève les mods de chasse et remet chaque élément mis de côté à sa place exacte. Si quelque
    chose a pris sa place entre-temps, cette chose est gardée dans DE_COTE\\conflits (rien n'est jamais effacé de
    tes fichiers). True si rien à faire ou si tout est revenu."""
    import json
    if not en_chasse():
        return True
    if fivem_ouvert():
        return False
    liste = json.load(open(_liste(), encoding="utf-8"))
    for rel in liste.get("ajoutes", []):
        p = os.path.join(FIVEM_APP, rel)
        if os.path.isfile(p):
            os.remove(p)                           # copie faite par mousquet depuis mods_chasse
    for rel in liste.get("deplaces", []):
        a, b = os.path.join(FIVEM_APP, rel), os.path.join(DE_COTE, rel)
        if not os.path.exists(b):
            continue                               # jamais déplacé (panne juste avant) : il est resté en place
        if os.path.exists(a):
            c = os.path.join(DE_COTE, "conflits", rel)
            os.makedirs(os.path.dirname(c), exist_ok=True)
            os.rename(a, c)
            noter(f"chasse : {rel} avait été recréé pendant la chasse, gardé dans conflits")
        os.makedirs(os.path.dirname(a), exist_ok=True)
        os.rename(b, a)
    manquants = [rel for rel in liste.get("deplaces", []) if not os.path.exists(os.path.join(FIVEM_APP, rel))]
    if manquants:
        noter("ATTENTION chasse : pas revenus : " + ", ".join(manquants))
        return False
    os.remove(_liste())
    for dossier in ("mods", "plugins"):
        try:
            os.rmdir(os.path.join(DE_COTE, dossier))
        except OSError:
            pass
    try:
        os.rmdir(DE_COTE)
    except OSError:
        pass
    noter(f"chasse finie : tes mods et ton pack sont revenus ({len(liste.get('deplaces', []))} élément(s))")
    return True


def remettre_tout():
    """Remet tes mods, ton pack et tes réglages (FiveM fermé). True si tout est revenu."""
    ok = remettre_de_cote()
    return remettre_reglages() and ok


def lancer_fivem():
    import subprocess
    subprocess.Popen([FIVEM_EXE], cwd=os.path.dirname(FIVEM_EXE), close_fds=True,
                     creationflags=0x00000008 | 0x00000200)     # détaché : il vit sa vie si mousquet se ferme


def gardien(parent=None):
    """Lancé en arrière-plan quand mousquet se ferme pendant la chasse : attend que mousquet soit bien fermé, puis
    que FiveM se ferme, et remet tout. S'arrête si mousquet est relancé entre-temps (c'est lui qui s'en occupe)."""
    if parent:
        h = k32.OpenProcess(0x00100000, False, parent)     # SYNCHRONIZE : attendre la fin de ce mousquet-là
        if h:
            k32.WaitForSingleObject(h, 30000)
            k32.CloseHandle(h)
    while en_chasse() or os.path.exists(chemins_reglages()[2]):
        h = k32.OpenEventW(0x00100000, False, EVENEMENT)
        if h:
            k32.CloseHandle(h)
            return
        if remettre_tout():
            return
        time.sleep(3)


_gardien_lance = [False]


def lancer_gardien():
    """Un seul gardien par mousquet : deux qui remettent en même temps se marcheraient dessus."""
    import subprocess
    if _gardien_lance[0]:
        return
    _gardien_lance[0] = True
    py = os.path.join(os.path.dirname(sys.executable), "pythonw.exe")
    if not os.path.exists(py):
        py = sys.executable
    subprocess.Popen([py, os.path.abspath(__file__), "--gardien", str(os.getpid())], cwd=ICI, close_fds=True,
                     creationflags=0x00000008 | 0x00000200 | 0x08000000)

# ============================ STYLE ===============================
# Thème « Cayo Perico », celui de la bannière : nuit noire, or de la lune, violet de la panthère, rose de la fleur,
# vert des feuilles, gris des rochers. Couleurs en R, G, B.
FOND = (0, 0, 0)
TEXTE = (236, 228, 208)
TERNE = (112, 104, 86)
MOYEN = (170, 158, 128)
ACCENT = (224, 183, 80)
OR = (246, 214, 132)
ROSE = (212, 46, 129)
VIOLET = (92, 94, 186)
BON = (60, 168, 96)
REPARE = (242, 196, 109)
MAUVAIS = (226, 64, 64)
TRAIT = (62, 50, 22)
POINTILLE = (56, 50, 38)
PISTE = (48, 40, 22)
LOGO = (VIOLET, ROSE, ACCENT)     # dégradé du logo : panthère, fleur, lune
LARGEUR = 46
COLONNES, RANGS = 52, 25
ESC = "\x1b"
RESET = ESC + "[0m"
VT = False

R_LOGO, R_TRAIT, R_LAB_CAN, R_CAN, R_PISTE = 2, 4, 6, 7, 11
R_LAB_PREP, R_PREP, R_LAB_CHASSE, R_ETAT, R_INFO, R_PIED = 13, 14, 20, 21, 22, 24


def bgr(c):
    return c[0] | (c[1] << 8) | (c[2] << 16)


def console_prete():
    """Couleurs ANSI 24 bits + fond et texte de toute la fenêtre repeints aux couleurs du thème."""
    h = k32.GetStdHandle(-11)
    mode = wt.DWORD()
    vt = bool(k32.GetConsoleMode(h, ctypes.byref(mode)) and k32.SetConsoleMode(h, mode.value | 4))
    if vt:
        i = CSBIEX()
        i.cbSize = ctypes.sizeof(CSBIEX)
        if k32.GetConsoleScreenBufferInfoEx(h, ctypes.byref(i)):
            i.ColorTable[0], i.ColorTable[7], i.wAttributes = bgr(FOND), bgr(TEXTE), 0x07
            # fenêtre et tampon à la taille exacte de l'interface, sans barre de défilement
            # (Get rend la fenêtre bornes comprises, Set la lit bornes exclues)
            i.dwSize = COORD(COLONNES, RANGS)
            i.srWindow = SMALL_RECT(0, 0, COLONNES, RANGS)
            i.dwMaximumWindowSize = COORD(COLONNES, RANGS)
            k32.SetConsoleScreenBufferInfoEx(h, ctypes.byref(i))
    return vt


def ink(t, c):
    return f"{ESC}[38;2;{int(c[0])};{int(c[1])};{int(c[2])}m{t}" if VT else t


def melange(a, b, k):
    k = max(0.0, min(1.0, k))
    return tuple(a[i] + (b[i] - a[i]) * k for i in range(3))


def grad(t, teintes):
    """Dégradé caractère par caractère à travers plusieurs teintes."""
    def teinte(k):
        k *= len(teintes) - 1
        i = min(int(k), len(teintes) - 2)
        return melange(teintes[i], teintes[i + 1], k - i)
    return "".join(ink(ch, teinte(i / max(1, len(t) - 1))) for i, ch in enumerate(t))


def label(t, droite=""):
    return "   " + ink(t, TERNE) + " " * max(1, LARGEUR - len(t) - len(droite)) + ink(droite, TERNE)


def ligne_diag(nom, detail, etat="ok"):
    detail = detail[:26]
    marque = {"fix": ink("  fix", REPARE), "warn": ink("   !!", MAUVAIS)}.get(etat, ink("    √", BON))
    return ("   " + ink(nom, TEXTE) + " " + ink("·" * max(1, 12 - len(nom)), POINTILLE) + " "
            + ink(f"{detail:<26}", MOYEN) + marque)


class Ecran:
    def __init__(self):
        self.morceaux = []

    def ecrire(self, rang, texte):
        self.morceaux.append(f"{ESC}[{rang};1H{texte}{RESET}{ESC}[K" if VT else "")

    def envoyer(self):
        if VT and self.morceaux:
            sys.stdout.write("".join(self.morceaux))
            sys.stdout.flush()
        self.morceaux = []


def banniere(e, sous_titre):
    heure = datetime.now().strftime("%d/%m %H:%M")
    e.ecrire(R_LOGO, "   " + grad("▄ ▄ ▄", LOGO) + "  " + ink("mousquet", ACCENT)
             + " " * (LARGEUR - 5 - 2 - 8 - len(heure)) + ink(heure, TERNE))
    e.ecrire(R_LOGO + 1, "   " + grad("▄███▄", LOGO) + "  " + ink(sous_titre, TERNE))
    e.ecrire(R_TRAIT, "   " + ink("─" * LARGEUR, TRAIT))


class Canopee:
    """Quatre rangs de jungle qui bougent au vent, des lucioles : feuilles vertes, fleurs roses, rochers gris."""
    TONS = [((19, 90, 48), (56, 156, 86)), ((19, 80, 44), (50, 136, 78)),
            ((120, 26, 76), (212, 46, 129)), ((86, 86, 92), (172, 172, 177))]

    def __init__(self):
        self.r = random.Random(11)
        pools = ["♣♣♠♣♠♣ ", "♣♠ψ♣ ♠  ", "  ψ  ║   ψ  ", "¥ψ,`.¥ ,ψ   "]
        self.cases = [[self.r.choice(p) for _ in range(LARGEUR)] for p in pools]
        self.lucioles = [[self.r.uniform(0, LARGEUR - 1), self.r.uniform(1, 3), self.r.uniform(0, 6.3)] for _ in range(3)]

    def rangs(self, t):
        luc = {}
        for l in self.lucioles:
            l[0] = (l[0] + self.r.uniform(-0.5, 0.5)) % LARGEUR
            l[1] = min(3.0, max(1.0, l[1] + self.r.uniform(-0.25, 0.25)))
            luc[(int(round(l[1])), int(l[0]))] = 0.5 + 0.5 * math.sin(t * 4 + l[2])
        sortie = []
        for y in range(4):
            morceaux = []
            for x in range(LARGEUR):
                ch = self.cases[y][x]
                vent = 0.6 + 0.4 * math.sin(t * 1.6 - x * 0.32 + y * 0.8)
                if y < 2 and ch in "♣♠" and math.sin(t * 2.3 + x * 1.7 + y) > 0.93:
                    ch = "♠" if ch == "♣" else "♣"
                c = melange(*self.TONS[y], vent)
                if ch == "║":
                    c = melange((70, 56, 34), (128, 100, 62), vent)
                if (y, x) in luc and ch == " ":
                    ch, c = "·", melange(FOND, OR, luc[(y, x)])
                morceaux.append(" " if ch == " " else ink(ch, c))
            sortie.append("   " + "".join(morceaux))
        return sortie


# ============================ PROGRAMME ============================


def ecran_fin(e, titre, detail):
    sys.stdout.write(f"{ESC}[2J{ESC}[?25l" if VT else "")
    banniere(e, "cayo perico · mode chasse")
    e.ecrire(R_LAB_CAN, "   " + ink("○", MOYEN) + " " + ink(titre, TEXTE))
    e.ecrire(R_LAB_CAN + 1, "   " + ink(detail, TERNE))
    e.envoyer()


RAPPEL_CONSOLE = ctypes.WINFUNCTYPE(wt.BOOL, wt.DWORD)
k32.SetConsoleCtrlHandler.argtypes = [RAPPEL_CONSOLE, wt.BOOL]


def garde_croix():
    """Fenêtre fermée avec la croix (ou session fermée) pendant la chasse : Windows coupe mousquet sans passer par la
    fin normale. Le gardien est lancé tout de suite ; il attend que ce mousquet soit coupé, puis remet tout."""
    def rappel(evenement):
        if evenement in (2, 5, 6) and (en_chasse() or os.path.exists(chemins_reglages()[2])):
            try:
                lancer_gardien()
            except Exception:
                pass
        return False
    f = RAPPEL_CONSOLE(rappel)
    k32.SetConsoleCtrlHandler(f, True)
    return f                                       # à garder : sans référence, Python libère le rappel


def main():
    global VT
    VT = console_prete()
    e = Ecran()
    croix = garde_croix()

    # une relance pendant la chasse ferme le mousquet en cours (un gardien remettra tout à la fermeture de FiveM)
    h = k32.OpenEventW(0x0002 | 0x100000, False, EVENEMENT)
    if h:
        k32.SetEvent(h)
        k32.CloseHandle(h)
        ecran_fin(e, "mousquet fermé", "tout revient quand FiveM se ferme")
        noter("fermé par la relance")
        time.sleep(1.6)
        return
    stop = k32.CreateEventW(None, True, False, EVENEMENT)
    try:
        chasser(e, stop)
    finally:
        k32.SetConsoleCtrlHandler(croix, False)
        k32.CloseHandle(stop)


def chasser(e, stop):
    sys.stdout.write(f"{ESC}[2J{ESC}[?25l" if VT else "")
    canopee = Canopee()
    t0 = time.monotonic()
    banniere(e, "cayo perico · mode chasse")
    e.ecrire(R_LAB_CAN, label("canopée"))
    e.ecrire(R_LAB_PREP, label("préparation"))
    e.envoyer()

    etapes, faites = 5, [0]

    def animer(duree):
        fin = time.monotonic() + duree
        while True:
            for i, r in enumerate(canopee.rangs(time.monotonic() - t0)):
                e.ecrire(R_CAN + i, r)
            f = round(38 * faites[0] / etapes)
            e.ecrire(R_PISTE, "   " + ink("━" * f, ACCENT) + ink("─" * (38 - f), PISTE) + ink(f"  {faites[0]}/{etapes}", MOYEN))
            e.envoyer()
            if time.monotonic() >= fin:
                break
            time.sleep(0.06)

    def verifier(rang, nom, fn):
        try:
            detail, etat = fn()
        except Exception as ex:
            detail, etat = str(ex) or type(ex).__name__, "warn"
        animer(0.35)
        faites[0] += 1
        e.ecrire(R_PREP + rang, ligne_diag(nom, detail, etat))
        animer(0.05)
        if etat != "ok":
            noter(f"{nom} : {detail} ({etat})")
        return etat, detail

    def demander(rang, nom, question, delai, avertissement=()):
        """Question o/n sur la ligne de l'étape (avertissement en dessous) ; sans réponse à temps, c'est non."""
        e.ecrire(R_PREP + rang, "   " + ink(nom, TEXTE) + " " + ink("·" * max(1, 12 - len(nom)), POINTILLE) + " "
                 + ink(question, ROSE))
        for i, l in enumerate(avertissement):
            e.ecrire(R_ETAT + i, "   " + ink(l, MAUVAIS if i == 0 else TEXTE))
        e.envoyer()
        while msvcrt.kbhit():
            msvcrt.getwch()
        fin, oui = time.monotonic() + delai, False
        while time.monotonic() < fin:
            if msvcrt.kbhit():
                oui = msvcrt.getwch().lower() == "o"
                break
            time.sleep(0.05)
        for i in range(len(avertissement)):
            e.ecrire(R_ETAT + i, "")
        e.envoyer()
        return oui

    ctx = {"refuses": [], "mods": [], "deja": False}

    def v_fivem():
        if not os.path.exists(FIVEM_EXE):
            raise FileNotFoundError("FiveM introuvable")
        if fivem_ouvert():
            if en_chasse():                        # mousquet relancé pendant une chasse : il reprend la garde
                ctx["deja"], ctx["mods"] = True, mods_ajoutes()
                return "chasse déjà en cours", "ok"
            raise RuntimeError("ouvert · ferme-le, relance")
        if not remettre_tout():                    # une chasse d'avant pas remise (panne) : remise d'abord
            raise RuntimeError("remise ratée · voir le log")
        return "trouvé · fermé", "ok"

    def v_mods_chasse():
        tous = mods_de_chasse()
        if not tous:
            return "aucun dans mods_chasse", "ok"
        for m in tous:                             # chaque mod est demandé, rien n'est mis sans un « o »
            nom, effet = presentation(m)
            if not demander(1, "mods chasse", f"{nom} ? ban possible · o/n", 20.0, (
                    f"⚠ {nom} : {effet}, un avantage." if effet else f"⚠ {nom} : un mod ajouté au jeu.",
                    "beaucoup de serveurs l'interdisent : tu peux",
                    "être banni. o = je le mets, n = sans (défaut).")):
                ctx["refuses"].append(m)
        mis = len(tous) - len(ctx["refuses"])
        return (f"{mis}/{len(tous)} mis · ban possible" if mis else "aucun mis"), "ok"

    def v_mods():
        ctx["mods"] = mods_de_chasse(ctx["refuses"])
        n = mettre_de_cote(ctx["mods"])
        return f"{n} élément{'s' if n > 1 else ''} mis de côté", "ok"

    def v_graphismes():
        mettre_reglages_chasse()
        return "au plus bas", "ok"

    def v_lancement():
        lancer_fivem()
        noter(f"FiveM lancé en mode chasse · mods : {', '.join(ctx['mods']) or 'aucun'}"
              + (f" · refusés : {', '.join(ctx['refuses'])}" if ctx["refuses"] else ""))
        return "FiveM lancé", "ok"

    animer(0.4)
    etat, detail = verifier(0, "fivem", v_fivem)
    if etat != "ok":
        raise RuntimeError(detail)
    if ctx["deja"]:
        for rang, nom in enumerate(("mods chasse", "mods", "graphismes", "lancement"), 1):
            faites[0] += 1
            e.ecrire(R_PREP + rang, ligne_diag(nom, "déjà fait"))
        animer(0.2)
    else:
        try:
            for rang, nom, fn in ((1, "mods chasse", v_mods_chasse), (2, "mods", v_mods), (3, "graphismes", v_graphismes),
                                  (4, "lancement", v_lancement)):
                etat, detail = verifier(rang, nom, fn)
                if etat != "ok":
                    raise RuntimeError(f"{nom} : {detail}")
        except BaseException:
            try:                                   # préparation cassée, FiveM pas lancé : rien ne reste de côté
                remettre_tout()
            except Exception as ex:
                noter("remise : " + "".join(traceback.format_exception_only(ex)).strip())
            raise

    e.ecrire(R_LAB_CHASSE, label("chasse"))
    e.ecrire(R_PIED, "   " + ink("●", BON) + " " + ink("on", TEXTE) + ink(" · q quitte · tout revient à la fermeture", TERNE))
    debut = time.monotonic()
    vu, absent, ouvert = ctx["deja"], None, False
    raison, prochain, prochaine_ui = None, 0.0, 0.0
    revenu = False
    try:
        while raison is None:
            if k32.WaitForSingleObject(stop, 0) == 0:
                raison = "relance"
                break
            if msvcrt.kbhit() and msvcrt.getwch().lower() in ("q", "\x1b"):
                raison = "q"
                break
            now = time.monotonic()
            if now >= prochain:
                prochain = now + 1.0
                ouvert = fivem_ouvert()
                if ouvert:
                    vu, absent = True, None
                else:
                    absent = absent or now
                    if vu and now - absent > ATTENTE_FERMETURE:
                        raison = "FiveM fermé"
                    elif not vu and now - debut > ATTENTE_LANCEMENT:
                        raison = "FiveM jamais apparu"
            if now >= prochaine_ui:
                prochaine_ui = now + 0.1
                t = now - t0
                for i, r in enumerate(canopee.rangs(t)):
                    e.ecrire(R_CAN + i, r)
                if ouvert:
                    p = int(t * 14) % (2 * (LARGEUR - 6))
                    p = p if p < LARGEUR - 6 else 2 * (LARGEUR - 6) - p
                    piste = (ink("─" * p, PISTE) + ink("━━━━━━", melange(PISTE, ACCENT, 0.7))
                             + ink("─" * (LARGEUR - 6 - p), PISTE))
                    pouls = melange(TERNE, ACCENT, 0.5 + 0.5 * math.sin(t * 3))
                    etat = ink("●", pouls) + " " + ink("en chasse", TEXTE) + ink(" · FiveM ouvert", TERNE)
                elif not vu:
                    piste = ink("─" * LARGEUR, PISTE)
                    etat = ink("○", MOYEN) + " " + ink("FiveM se lance", TEXTE) + ink(" ." * (1 + int(t * 2) % 3), TERNE)
                else:
                    piste = ink("─" * LARGEUR, PISTE)
                    reste = max(0.0, ATTENTE_FERMETURE - (now - absent))
                    etat = ink("○", OR) + " " + ink("FiveM fermé", TEXTE) + ink(f" · je remets tout dans {reste:.0f} s", TERNE)
                e.ecrire(R_PISTE, "   " + piste)
                e.ecrire(R_ETAT, "   " + etat)
                mods = ", ".join(presentation(m)[0] for m in ctx["mods"]) or "aucun"
                mods = mods if len(mods) <= 18 else mods[:17] + "…"     # la ligne tient dans les 52 colonnes
                e.ecrire(R_INFO, "   " + ink("depuis", TEXTE) + " " + ink("·" * 6, POINTILLE) + " "
                         + ink(f"{int((now - debut) // 60)} min", MOYEN) + ink(f"  ·  mods : {mods}", TERNE))
                e.envoyer()
            time.sleep(0.05)
    finally:
        try:
            revenu = remettre_tout()
        except Exception as ex:
            noter("remise : " + "".join(traceback.format_exception_only(ex)).strip())
        if not revenu:
            lancer_gardien()                       # FiveM encore ouvert : le gardien remettra tout à sa fermeture
    noter(f"mousquet fermé ({raison}) · " + ("tout est revenu" if revenu else "le gardien remettra tout"))
    if revenu:
        ecran_fin(e, "chasse finie · tout est revenu", "tes mods, ton pack et tes réglages sont en place")
    else:
        ecran_fin(e, "mousquet fermé", "tout revient quand FiveM se ferme")
    time.sleep(2.5)


if __name__ == "__main__":
    if "--gardien" in sys.argv:
        i = sys.argv.index("--gardien")
        parent = int(sys.argv[i + 1]) if len(sys.argv) > i + 1 and sys.argv[i + 1].isdigit() else None
        try:
            gardien(parent)
        except Exception as ex:
            noter("gardien : " + "".join(traceback.format_exception_only(ex)).strip())
        sys.exit(0)
    try:
        main()
    except KeyboardInterrupt:
        pass
    except Exception as ex:
        noter("ERREUR " + "".join(traceback.format_exception(ex)).strip().replace("\n", " | "))
        sys.stdout.write(f"{ESC}[{R_PIED - 1};1H{ESC}[J" if VT else "\n")
        print("   " + ink(f"x  {ex}", MAUVAIS) + RESET)
        print("   " + ink("entrée pour fermer", TERNE) + RESET)
        msvcrt.getwch()
    finally:
        sys.stdout.write(f"{ESC}[?25h{RESET}" if VT else "")
        sys.stdout.flush()
