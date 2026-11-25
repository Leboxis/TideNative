# TideNative Desktop Player — Page Override (Tauri v2 + React 19 120 Hz)

> Surcharge spécifique de `MASTER.md` pour le shell desktop Windows (`app-tauri` + `ui/`).

## Layout Desktop Principal (`AppShell`)

- **Fenêtre frameless OLED (`#040406`)** : `visible: false` au démarrage jusqu'au premier rendu React prêt (zéro flash blanc).
- **Navigation supérieure flottante (`TopNavPill`)** : historique Précédent / Suivant, onglets principaux (**Écouter**, **Explorer**, **Favoris**, **Collection**), accès recherche instantanée (`Ctrl+K` / `/`), Paramètres Audio, statut session.
- **Zone principale (scroll unique)** :
  - Vues catalogue et détail (`DiscoverView`, `ExploreView`, `CollectionView`, `FavoritesView`, `AlbumDetailView`, `ArtistDetailView`, `PlaylistDetailView`, `MixDetailView`, `AudioSettingsView`).
  - En-têtes de vues détail habillés par `DetailHeaderSurface` (halo teinté par la pochette).
  - Listes de pistes virtualisées via `VirtualTrackList` (TanStack Virtual) sans double scroll imbriqué dans les pages composites.
- **Barre de lecture flottante (`PlayerBar`)** :
  - Gauche : pochette cliquable (ouvre `NowPlayingView`), titre, artiste cliquable, cœur favori.
  - Centre : transport complet (Aléatoire, Précédent, Lecture/Pause, Suivant, Répétition 3 états `Off` / `All` / `One`) + `Scrubber` 120 Hz (`MotionValue` `scaleX`, zéro `setState` par frame).
  - Droite : badge qualité temps réel (`MAX` `#FFD432`, `HIGH` `#33F3FF`, `LOW` `#A8A8B3`, `Bit-Perfect` `#30D158`), volume / verrou Bit-Perfect, tiroir File d'attente (`Q`), Mode compact (`C`), Plein écran (`N`).
- **Mode Plein écran (`NowPlayingView`)** : fond immersif `AmbientBackdrop`, pochette grand format + `MatrixOrbCanvas`, panneau droit à 3 onglets (**Paroles** avec `.lyrics-fade-mask` et auto-scroll intelligent, **À suivre**, **Crédits**).
- **Mode Compact (`CompactPlayer`)** : mini-lecteur desktop basculable via `C` et refermable via `Escape`.

## Discipline d'Interaction & Raccourcis

- `Space` : Lecture / Pause (ignoré dans les champs de saisie et boutons focusés)
- `←` / `→` : Avancer / reculer de 10 s (`Ctrl+←` / `Ctrl+→` : piste précédente / suivante)
- `↑` / `↓` : Volume ±5 % (désactivé si verrou Bit-Perfect actif)
- `M` : Couper / rétablir le son
- `Ctrl+K` ou `/` : Palette de recherche rapide
- `Q` : File d'attente · `N` : Plein écran · `C` : Mode compact · `?` : Mémo des raccourcis
- `Alt+←` / `Alt+→` et boutons latéraux de la souris : Historique Précédent / Suivant
- `Escape` : Ferme les modales, tiroirs et modes plein écran / compact
