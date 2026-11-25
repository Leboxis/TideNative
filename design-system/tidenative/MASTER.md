# TideNative — Design System Master File (OLED Spatial Immersion)

> **Source de vérité design** pour l'application desktop **TideNative** (Tauri v2 + React 19 à 120 Hz + Cœur Rust WASAPI Exclusif Bit-Perfect).
> Les variables CSS actives sont définies dans `ui/src/index.css` (`@theme`) et les tokens DTCG dans `design-system/tidenative/tokens.json`.

---

**Project:** TideNative  
**Direction artistique:** OLED Spatial Immersion (audiophile natif Windows, inspiré TIDAL / Roon / Apple Music)  
**Stack UI:** React 19, TypeScript, Tailwind CSS v4, shadcn/ui (Radix UI), Motion (`motion/react`), TanStack Virtual, Lucide Icons  
**Performance cible:** 120 Hz natif, zéro `setState` par frame, zéro flash blanc au démarrage (`#040406`)

---

## 1. Palette OLED Spatial Immersion

### Surfaces & Bordures

| Rôle | Valeur | Variable CSS | Usage |
|------|--------|--------------|-------|
| Canvas OLED | `#040406` | `--color-canvas` | Fond fenêtre principal (zéro flash blanc au démarrage) |
| Ambient Top | `#0A0B0F` | `--color-ambient-top` | Dégradé supérieur de l'arrière-plan ambiant |
| Elevated Card | `#111116` | `--color-elevated` | Cartes (`.card-oled`), surfaces d'en-têtes détail, inputs |
| Elevated Hover | `#17171F` | `--color-elevated-hover` | État survol sur cartes et contrôles de surface |
| Surface Tint | `#0B0D14` | `--color-surface-tint` | Teinte neutre de repli pour `AmbientBackdrop` |
| Glass Surface | `rgba(18, 18, 24, 0.84)` | `--color-glass-surface` | Barre de lecture, panneaux flottants, pilules d'action (`.glass-surface`) |
| Glass Pill | `rgba(24, 24, 30, 0.88)` | `--color-glass-pill` | Navigation supérieure `TopNavPill` et tooltips |
| Hairline Border | `rgba(255, 255, 255, 0.08)` | `--color-hairline` | Séparateurs 1 px haute précision |
| Specular Border | `rgba(255, 255, 255, 0.13)` | `--color-specular` | Reflet spéculaire supérieur sur surfaces verre |
| Strong Border | `rgba(255, 255, 255, 0.22)` | `--color-strong-border` | Survol de cartes et états actifs |

### Texte & Contraste (WCAG AA / AAA sur `#040406`)

| Rôle | Valeur | Variable CSS | Usage |
|------|--------|--------------|-------|
| Ink Primary | `#F5F5F7` | `--color-ink-primary` | Titres, noms de pistes, contrôles primaires |
| Ink Secondary | `rgba(235, 235, 245, 0.70)` | `--color-ink-secondary` | Artistes, albums, métadonnées secondaires |
| Ink Muted | `rgba(235, 235, 245, 0.58)` | `--color-ink-muted` | Numéros de piste, durées, légendes (contraste ≥ 4.5:1) |

### Badges Qualité Officiels TIDAL & Signaux Audio

| Rôle | Valeur | Variable CSS | Usage |
|------|--------|--------------|-------|
| TIDAL MAX | `#FFD432` | `--color-tidal-max` | Hi-Res FLAC (24-bit / jusqu'à 192 kHz) |
| TIDAL HIGH / Accent | `#33F3FF` | `--color-tidal-high` | Lossless CD 16-bit/44.1 kHz, piste active, anneau de focus |
| TIDAL LOW | `#A8A8B3` | `--color-tidal-low` | AAC compressé |
| Bit-Perfect | `#30D158` | `--color-bit-perfect` | WASAPI Exclusif Bit-Perfect verrouillé |
| Favorite | `#FF2D55` | `--color-favorite` | Cœur favori actif (uniquement lorsqu'aimé) |
| Signal Danger | `#FF453A` | `--color-signal-danger` | Erreurs réseau/DAC, actions destructives |

---

## 2. Typographie

Les polices sont **auto-hébergées localement** (`ui/public/fonts/*.woff2`, zéro dépendance réseau au rendu) et sélectionnables dans les Paramètres Audio via `html[data-ui-font]`:

- **Default:** `Inter` (`--font-sans`)
- **SeasonMix:** `Season Mix` (variable font locale)
- **DmSans:** `DM Sans`
- **PlusJakartaSans:** `Plus Jakarta Sans`
- **Monospace technique:** `JetBrains Mono` / `Cascadia Code` (`--font-mono`) pour fréquences d'échantillonnage, bit-depth, temps (`tabular-nums`) et raccourcis clavier (`<kbd>`).

---

## 3. Composants & Surfaces Signature

- **`AmbientBackdrop` (`ui/src/components/layout/AmbientBackdrop.tsx`) :** double calque GPU en crossfade `opacity` (650 ms, `cubic-bezier(0.22, 1, 0.36, 1)`) alimenté par `CoverPalette` extraite de la pochette en cours.
- **`DetailHeaderSurface` (`ui/src/components/media/DetailHeaderSurface.tsx`) :** en-têtes d'albums, artistes, playlists et mix teintés par la pochette courante (`deriveHeaderTint` depuis `ui/src/lib/covers.ts`) avec halo radial double + filet spéculaire supérieur, tout en préservant le contraste du texte blanc.
- **`MediaCard` (`ui/src/components/media/MediaCard.tsx`) :** carte OLED (`.card-oled`) avec miniature `tidal-cover://` et bouton Play rapide au survol / focus clavier (`quickPlayAlbum` / `quickPlayPlaylist`).
- **`ActionTooltip` (`ui/src/components/ui/tooltip.tsx`) :** tooltips Radix homogènes avec badges de raccourcis clavier (`<kbd>`).
- **`Skeleton` (`ui/src/components/ui/skeleton.tsx`) :** `DetailHeaderSkeleton`, `TrackListSkeleton`, `CardGridSkeleton` pour supprimer les états texte `"Chargement…"`.
- **`NowPlayingView` (`ui/src/components/player/NowPlayingView.tsx`) :** visualiseur `MatrixOrbCanvas` (`Float32Array(5)` dans `requestAnimationFrame`), paroles synchronisées avec `.lyrics-fade-mask` (fondu haut/bas 12 %) et auto-scroll intelligent qui se met en pause lors du défilement manuel avec bouton « Revenir à la ligne en cours ».

---

## 4. Règles 120 Hz, Motion & Accessibilité

1. **Zéro `setState` par frame** : le `Scrubber` pilote un `MotionValue` (`scaleX`), `MatrixOrbCanvas` lit `telemetryRuntime` directement dans `requestAnimationFrame`.
2. **Transitions GPU uniquement** : animer `opacity` et `transform`, éviter `transition-all` ou les changements de layout au survol.
3. **`prefers-reduced-motion: reduce`** : supporté globalement dans `ui/src/index.css` et dans l'auto-scroll des paroles (`behavior: "auto"`).
4. **Accessibilité clavier & souris** :
   - Contour `:focus-visible` `2px solid var(--color-tidal-high)`.
   - `cursor-pointer` sur tous les contrôles cliquables.
   - Raccourcis globaux (`Space`, `←`/`→`, `↑`/`↓`, `Ctrl+K` / `/`, `M`, `Q`, `N`, `C`, `?`, `Alt+←`/`→`, boutons latéraux souris) documentés dans `ShortcutsDialog`.
5. **Zéro emoji dans l'UI** : uniquement des icônes vectorielles `lucide-react`.
