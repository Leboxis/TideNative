# TideNative Desktop Player — Page Override (`iced` + Windows 11 Mica)

> Surcharge spécifique de `MASTER.md` pour l'application desktop native Windows 11 (`crates/app`).

## Palette Audiophile Dark (WCAG AA/AAA vérifié)

| Token Rust | Hex | Contraste sur `#0B0D14` | Usage |
|---|---|---|---|
| `BG_CANVAS` | `#0B0D14` | — | Fond fenêtre principal (semi-transparent sous Mica Win11) |
| `BG_SIDEBAR` | `#0F121C` | — | Rail de navigation gauche (232 px fixe) |
| `BG_SURFACE` | `#151926` | — | Cartes, lignes survolées, panneaux de paramètres |
| `BG_ELEVATED` | `#1C2233` | — | Barre de lecture dockée, popovers, modale connexion |
| `BORDER_SUBTLE` | `#242B40` | — | Séparateurs 1 px haute précision |
| `BORDER_FOCUS` | `#38BDF8` | `9.2:1` | Contour focus clavier 2 px visible |
| `TEXT_PRIMARY` | `#F4F6FB` | `17.5:1` | Titres de pistes, nom d'album, temps actif |
| `TEXT_SECONDARY` | `#9CA8C2` | `7.8:1` | Artistes, colonnes secondaires, sous-titres |
| `TEXT_MUTED` | `#6B7691` | `4.6:1` | Numéros de piste, horodatages secondaires |
| `ACCENT_CYAN` | `#00E5FF` | `12.9:1` | Action primaire, piste en cours, curseur seek |
| `SIGNAL_BIT_PERFECT` | `#22C55E` | `9.6:1` | Badge `WASAPI Exclusif • Bit-Perfect` |
| `SIGNAL_HIRES` | `#F59E0B` | `10.1:1` | Badge `MAX / 24-bit 192 kHz` |
| `SIGNAL_SHARED` | `#FB923C` | `8.8:1` | Avertissement repli `WASAPI Partagé` |
| `SIGNAL_DANGER` | `#EF4444` | `5.3:1` | Erreurs réseau / DAC / déconnexion |

## Hiérarchie Spatiale & Rythme 4/8 px

- **Sidebar gauche (`232 px`)** : Logo vectoriel TideNative, 4 vues principales (Accueil / Découvrir, Favoris, Playlists, Recherche), statut session TIDAL, accès Paramètres Audio.
- **Zone principale (fluide)** : En-tête de vue + champ recherche/filtre + **liste virtualisée** (`ROW_HEIGHT = 56.0 px`, fenêtrage exact sur `Viewport::absolute_offset().y` + overscan de 6 lignes) capable d'afficher 5 000+ pistes à 120 FPS sans lag.
- **Barre de lecture dockée (`88 px` fixe en bas)** :
  - Gauche (`30 %`) : Pochette 56×56 arrondie (`6 px`), titre 14 px SemiBold, artiste 12 px secondaire.
  - Centre (`40 %`) : Contrôles transport vectoriels SVG (Précédent, Lecture/Pause `40×40` tactile, Suivant) + barre de progression interactive avec temps écoulé / durée en chiffres tabulaires.
  - Droite (`30 %`) : **Badge Signal Path temps réel** (`FLAC • 24-bit / 192 kHz • WASAPI Exclusif • Bit-Perfect` avec pastille LED verte/ambre), sélecteur rapide DAC & verrou Bit-Perfect 0 dB / volume logiciel.

## Discipline d'Interaction (`emil-design-eng` + `ui-ux-pro-max/pro-rules.md`)

1. **Zéro emoji** dans la structure UI : toutes les icônes (Play, Pause, Next, Prev, Search, Heart, Disc, Settings, Lock, Volume, Speaker, Wave, Check, Alert) sont des **SVG vectoriels** inline cohérents (`stroke-width: 1.75`, `24×24 viewBox`).
2. **Raccourcis clavier sans latence (0 ms)** : `Space` (Lecture/Pause), `Ctrl+K` / `/` (Focus Recherche), `ArrowLeft` / `ArrowRight` (Seek ±10 s), `Ctrl+ArrowRight` / `Ctrl+ArrowLeft` (Piste suivante/précédente), `Escape` (Fermer modale/drawer).
3. **États visuels stables** : le hover/press sur une piste ou un bouton change uniquement la couleur/bordure (jamais de décalage de géométrie ni de layout shift).
4. **Accessibilité** : cibles cliquables confortables (≥ 36–44 px), indicateur textuel + couleur sur le badge Bit-Perfect (jamais la couleur seule), erreurs inline dans la modale de connexion PKCE.
