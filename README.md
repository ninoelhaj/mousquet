<h1 align="center">mousquet</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0B0B0E?style=flat-square&logo=windows&logoColor=white" alt="Windows">
  <img src="https://img.shields.io/badge/Python-3.12+-84DE78?style=flat-square&logo=python&logoColor=white" alt="Python 3.12+">
  <img src="https://img.shields.io/badge/d%C3%A9pendances-aucune-0B0B0E?style=flat-square" alt="Aucune dépendance">
  <img src="https://img.shields.io/badge/serveur-Baylife-84DE78?style=flat-square" alt="Baylife">
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-utilisation%20seule-0B0B0E?style=flat-square" alt="Licence : utilisation seule"></a>
</p>

**mousquet** prépare FiveM pour la chasse à Cayo Perico sur **Baylife**. Le temps de la chasse, ton FiveM tourne
« tout neuf » : sans tes mods, sans ton pack graphique, avec les graphismes au plus bas. Quand tu fermes FiveM, tout
revient exactement à sa place.

```
   ▄ ▄ ▄  mousquet                    16/11 13:48
   ▄███▄  jungle cayo · mode chasse
   ──────────────────────────────────────────────

   préparation
   fivem ······ trouvé · fermé                √
   lowbush ···· mis · ban possible            √
   mods ······· 15 éléments mis de côté       √
   graphismes · au plus bas                   √
   lancement ·· FiveM lancé                   √

   chasse
   ● en chasse · FiveM ouvert
   depuis ····· 12 min   ·   mods : lowbush.rpf

   ● on · q quitte · tout revient à la fermeture
```

> [!NOTE]
> mousquet ne touche ni au jeu ni à sa mémoire, et n'appuie sur aucune touche. Jeu fermé, il déplace des fichiers
> dans le dossier de FiveM et change tes réglages graphiques. C'est tout.

> [!WARNING]
> Les mods qui enlèvent des buissons (comme **lowbush**) donnent un avantage et sont interdits sur beaucoup de
> serveurs : **ban possible**. mousquet ne fournit aucun mod. Si tu en mets un, il te le redemande à chaque chasse,
> avec cet avertissement.

## 🚀 Démarrer

1. Installe [Python 3.12 ou plus](https://www.python.org/downloads/) (coche « Add python.exe to PATH »).
   mousquet n'a besoin d'aucune autre bibliothèque.
2. Télécharge le dépôt : bouton **Code** → **Download ZIP**, puis décompresse-le où tu veux.
3. **Ferme FiveM**, puis double-clique sur `mousquet.bat`.

mousquet prépare tout, lance FiveM, puis attend. Tu joues normalement. Quand tu fermes FiveM, il remet tout en place
au bout de 10 secondes, affiche « chasse finie · tout est revenu » et se ferme tout seul.

## 🌿 Ce qu'il fait

| Étape | Ce qui se passe |
|---|---|
| **fivem** | vérifie que FiveM est installé et fermé (ouvert, il refuse : les fichiers seraient verrouillés) |
| **lowbush** | si un mod « à risque » est dans `mods_chasse`, te demande `o` ou `n` (sans réponse en 20 s : non) |
| **mods** | déplace tout ce qui est dans `FiveM.app\mods` et `FiveM.app\plugins` (mods, ReShade, presets…) vers `FiveM\mis_de_cote_par_mousquet`, puis copie tes mods de chasse |
| **graphismes** | garde tes réglages dans `reglages\normal.xml` et met les plus bas du jeu (résolution, plein écran et densité de population ne bougent pas) |
| **lancement** | ouvre FiveM |

Les fichiers sont **déplacés**, pas copiés : c'est instantané et ça ne prend aucune place en plus. Une liste de chaque
élément est écrite **avant** chaque déplacement : même après une panne de courant, tout peut revenir.

Si tu changes des réglages graphiques pendant la chasse, ils sont gardés dans `reglages\chasse.xml` pour la
prochaine fois.

## 🎯 Mods de chasse

Mets les `.rpf` que tu veux pendant la chasse dans le dossier `mods_chasse`. Ils vont dans FiveM le temps de la
chasse, puis en ressortent. Seuls les `.rpf` sont pris : jamais de `.dll` ni de `.asi`, qui sont des programmes.

## 🛟 Si tu fermes mousquet avant FiveM

- **q**, **une relance** de `mousquet.bat` ou **la croix** de la fenêtre : mousquet se ferme et laisse un gardien
  discret, sans fenêtre. Il remet tout dès que FiveM se ferme.
- **FiveM ne s'ouvre jamais** (90 s) : tout est remis.
- **Panne** (PC éteint en pleine chasse) : relance `mousquet.bat`. Il remet d'abord tout ce qui est resté de côté.
- Si un fichier a été recréé par FiveM pendant la chasse à la place d'un des tiens, rien n'est écrasé : le nouveau
  est gardé dans `FiveM\mis_de_cote_par_mousquet\conflits`.

Tout ce que fait mousquet est noté dans `mousquet.log`.

## 📁 Fichiers

| Fichier | Rôle |
|---|---|
| `mousquet.bat` | lance mousquet dans sa console |
| `mousquet.py` | tout le programme (bibliothèque standard de Python seulement) |
| `mods_chasse\` | tes mods de chasse (`.rpf`) |
| `reglages\` | créé au premier lancement : tes réglages normaux et ceux de chasse |
| `mousquet.log` | créé au premier lancement : le journal |

## 📜 Licence

mousquet n'est pas un logiciel libre : tu peux l'installer et l'utiliser gratuitement, mais pas le modifier ni le
redistribuer. Voir [LICENSE](LICENSE).

<sub>Projet indépendant, sans lien avec l'équipe de Baylife ni avec Cfx.re.</sub>
