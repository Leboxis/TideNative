# TideNative — Design Architecture : Migration UI `iced 0.14` → Tauri v2 + React (120 Hz, Windows)

**Date :** 2026-11-23  
**Statut :** Proposition de design (en attente de validation avant planification)  
**Portée :** Remplacement complet de la couche interface/rendu (`app/` sous `iced 0.14`) par un frontend React moderne, sobre et premium à 120 Hz hébergé par Tauri v2, tout en conservant intégralement le cœur métier Rust natif.

---

## 1. Intention & Pourquoi

TideNative dispose déjà d'un cœur audio et métier Rust natif de très haut niveau (WASAPI Exclusif bit-perfect jusqu'à 24-bit / 192 kHz avec MMCSS `"Pro Audio"`, streaming MPEG-DASH parallèle, décodeur Symphonia, cache SQLite WAL/FTS5 via `winsqlite3.dll`, reprise à froid `< 10 ms` par snapshot, intégrations Windows 11 DWM/SMTC/Tray).

L'objectif de cette migration est de **remplacer uniquement la couche présentation (`iced 0.14`)** par une interface desktop **sobre, premium, parfaitement agencée et fluide à 120 Hz** construite avec des briques UI éprouvées (**Tailwind CSS v4 + shadcn/ui sur Radix UI + `@rare-ui` + Motion**), afin de débloquer :
- des micro-interactions, transitions et gestes irréprochables à 120 Hz (budget **8,33 ms / frame**) ;
- une mise en page haut de gamme reposant sur des primitives accessibles et des composants pré-conçus plutôt que du layout manuel ;
- un fond ambiant discret et dynamique réactif aux couleurs de la pochette en cours d'écoute ;
- une itération UI drastiquement plus rapide sans jamais compromettre le pipeline audio Rust natif.

---

## 2. Objectifs, Non-objectifs & Anti-objectifs

### Objectifs
- **Parité fonctionnelle complète** avec l'application `iced` actuelle (16 vues/panneaux/modales de `app/src/views/` : `discover`, `explore`, `collection`, `playlists`, `album_detail`, `artist_detail`, `tracklist`, `now_playing`, `queue_panel`, `player_bar`, `top_nav`, `activity`, `settings`, `login_modal`, `modals`, `matrix_orb`).
- **120 Hz réels** sur Windows 10/11 (WebView2 / DirectComposition) lors du défilement de grandes listes, des transitions de vues, du scrubbing, de l'ouverture de `NowPlaying` / `QueuePanel` et de l'animation `MatrixOrb`.
- **Ressenti 100 % natif Windows 11** : zéro flash blanc au démarrage, fenêtre frameless avec Snap Layouts + coins arrondis DWM, `minimize_to_tray`, SMTC (touches média + flyout Windows 11), raccourcis clavier natifs.
- **Zéro régression audio ou démarrage** : le pipeline WASAPI Exclusif bit-perfect, l'enchaînement gapless, ReplayGain, hot-plug DAC et la restauration à froid depuis `SnapshotStore` restent gérés 100 % en Rust.
- **Bascule directe sur Tauri** : ne pas maintenir le binaire `iced` compilable en parallèle.

### Non-objectifs
- Support macOS ou Linux (cible explicitement **Windows 10/11 uniquement**).
- Mode application web autonome ou télécommande réseau LAN (lecteur **desktop local uniquement**, pas de serveur Axum).
- Réécriture ou modification du moteur audio (`tidal-audio-wasapi`, `tidal-decoder`, `tidal-stream`, `tidal-player`) ou des clients API (`tidal-api`, `tidal-auth`).

### Anti-objectifs (ce qui constituerait un échec)
- Interface « page web déguisée » (flash blanc à l'ouverture, menu clic-droit navigateur, sélection de texte accidentelle, images déplaçables, scroll ou resize saccadé).
- Animations tape-à-l'œil ou effets de flou plein écran qui dégradent la lisibilité ou font chuter le framerate sous 120 Hz.
- Passage d'images en base64 ou de télémétrie audio 60/120 Hz en JSON par `invoke`/events (source de pauses GC et de jank).
- Perte des comportements métier existants (garde `playback_generation`, `seek_hold_until`, filtrage explicite/IA, scrobbling Last.fm, capture de session `listen.tidal.com`).

---

## 3. Contraintes techniques du dépôt

- **Toolchain Rust :** `stable-x86_64-pc-windows-gnu` ([rust-toolchain.toml](file:///C:/Users/KEDI-ADMIN/Documents/Project/tidenative/rust-toolchain.toml)), édition **2024**, `resolver = "3"`.
- **Dossier de compilation :** `target-dir = "C:/Users/KEDI-ADMIN/AppData/Local/tidenative-target"` ([.cargo/config.toml](file:///C:/Users/KEDI-ADMIN/Documents/Project/tidenative/.cargo/config.toml)) pour compatibilité MinGW-w64.
- **Validation précoce obligatoire :** Tauri v2 (`tauri`, `tauri-build`, `wry`) doit compiler proprement sur `stable-x86_64-pc-windows-gnu` dès l'Étape 1 (PoC), sans casser la liaison `webview2-com` ni `windows` v0.62 déjà utilisés dans `crates/platform-win`.
- **Distribution :** maintenir la compatibilité avec le packaging Windows portable/setup dans `installer/Build-Release-Dist.ps1` (un seul exécutable `TideNative.exe` embarquant le frontend buildé via Tauri).

---

## 4. Approches étudiées & Choix retenus

### Architecture globale
- **Option A — Tauri v2 + Vite + React + TypeScript (Retenue) :** SPA locale embarquée, démarrage rapide, HMR instantané, zéro friction sur les routes dynamiques TIDAL (`/album/:id`, `/artist/:id`, `/playlist/:uuid`), accès direct à `Channel<Raw>` et aux protocoles URI custom.
- **Option B — Tauri v2 + Next.js (`output: 'export'`) (Écartée) :** perd tous les bénéfices de Next (SSR/ISR/Server Actions/Image) dans une WebView locale et complique inutilement les routes dynamiques sur un catalogue infini.
- **Option C — Tauri v2 + serveur local Axum (Écartée) :** complexité réseau loopback (port dynamique, CORS, sécurité locale) injustifiée pour un lecteur strictement desktop local.

### Fond dynamique réactif aux couleurs de pochette
- **Option 1 — Mesh Shader WebGL2 temps réel (Écartée) :** plus complexe à maintenir et plus chargé visuellement pour un rendu voulu sobre.
- **Option 2 — Extraction palette côté Rust + halos ambiants pré-rendus + crossfade GPU `opacity` (Retenue) :**
  - **Sober & premium :** teinte discrètement la base OLED `#040406` sur les vues principales (halo supérieur doux ~15–20 % d'opacité) et s'ouvre en atmosphère plus profonde sur la vue plein écran `NowPlaying`, sans jamais nuire au contraste ni à la lisibilité.
  - **Coût 120 Hz quasi nul :** les couleurs dominantes (3 à 4 couleurs harmonisées + version sombre sécurisée pour le contraste) sont extraites une seule fois par pochette en Rust dans `CoverCache` (< 1 ms sur miniature décodée, mises en cache), peintes dans deux calques de gradient/canvas statiques alternés, puis fondues en ~900 ms via une simple transition GPU `opacity` (zéro `filter: blur()` animé en temps réel).

### Stratégie de bascule
- **Bascule directe (Retenue) :** on extrait la logique d'orchestration dans `crates/app-engine` et on branche le nouveau binaire `app-tauri/` + `ui/`. L'ancien dossier `app/` (`iced`) est retiré des membres compilés par défaut mais conservé temporairement en lecture seule comme référence métier jusqu'à parité complète, puis supprimé en fin de migration.

---

## 5. Architecture cible détaillée

### 5.1 Organisation du Workspace Cargo

Les 9 crates existantes restent intactes :
- `crates/core` (`tidal-core`) — modèles de domaine sérialisables (+ dérivation `specta::Type` derrière une feature optionnelle ou directe).
- `crates/tidal-auth`, `crates/tidal-api`, `crates/tidal-stream`, `crates/decoder`, `crates/audio-wasapi`, `crates/player`, `crates/cache`, `crates/platform-win`.

Nouvelles briques :
1. **`crates/app-engine`** (nouvelle crate Rust headless, sans aucune dépendance UI) :
   - Encapsule l'orchestration aujourd'hui présente dans `app/src/main.rs` : file d'attente (`queue`, `original_queue`, `queue_index`, `play_history`, shuffle, repeat, autoplay radio, `playback_generation`), reprise de session (`SnapshotStore`, `pending_resume_seek_ms`, `seek_hold_until`), synchronisation bibliothèque, recherche hybride (SQLite FTS5 local + API TIDAL avec annulation par séquence `search_seq`), filtrage explicite/IA & tri, scrobbling Last.fm, statistiques d'activité, pont `SmtcController` & `TrayController`.
   - Découpe son état interne en verrous granulaires (`PlayerHandle` lock-free, `Arc<RwLock<PlaybackSnapshot>>`, `Arc<RwLock<LibrarySnapshot>>`, `Arc<CoverCache>`) pour ne jamais bloquer l'IPC lors d'un appel réseau.
   - Expose des méthodes `async` propres et un flux d'événements sortants typés (`EngineEvent`).
2. **`app-tauri/`** (binaire final `TideNative`, Tauri v2) :
   - Instancie `Arc<AppEngine>` dans `tauri::State`.
   - Enregistre les commandes `#[tauri::command]` + événements via **`tauri-specta`** (génération automatique de `ui/src/bindings.ts`).
   - Enregistre le protocole asynchrone custom **`tidal-cover://localhost/{cover_id}/{size}`** directement branché sur `CoverCache` (flux JPEG binaire zéro-JSON, entêtes `Cache-Control: immutable`, extraction lazy de la palette dominante associée).
   - Ouvre un **`tauri::ipc::Channel<InvokeResponseBody::Raw>`** poussant les paquets binaires fixes de télémétrie audio (niveaux `[f32; 5]` de `tidal_audio_wasapi::live_audio_levels()`, `position_ms`, `duration_ms`, `buffered_ms`) à **60 Hz** uniquement lorsque la fenêtre est visible et en lecture/buffering (0 Hz dès pause ou `minimize_to_tray`).
   - Attache au `.setup()` le `HWND` natif Tauri à `apply_windows11_styling(hwnd, true)` (DWM dark mode, coins arrondis, teinte `#040406`), `SmtcController::attach(hwnd, ...)` et `TrayController` (ou `tauri::tray`).
   - Gère le login TIDAL via une **fenêtre secondaire Tauri (`WebviewWindow`)** avec `data_directory` dédié et hook `with_webview` sur `ICoreWebView2` pour capturer la session `listen.tidal.com` sans conflit de second `WebView2Loader.dll`.
3. **`ui/`** (Frontend Vite + React 19 + TypeScript strict) :
   - **Styling :** **Tailwind CSS v4** (`@theme` alimenté par la palette OLED Dark ci-dessous).
   - **Primitives & Templates UI pré-conçus :** **shadcn/ui** (composants accessibles basés sur **Radix UI** : `Dialog`, `DropdownMenu`, `ContextMenu`, `Popover`, `Tooltip`, `Tabs`, `Slider`, `Switch`, `Select`, `ScrollArea`, `Command` pour la recherche `Ctrl+K`), icônes **Lucide**, notifications **Sonner**, et **`@rare-ui/matrix-orb`** pour l'orbe audio temps réel.
   - **Animations :** **Motion (`motion/react`)** (version >= 14.2 pour accélération WAAPI matérielle des transforms).
   - **État & Listes :** **Zustand** (stores découpés : `usePlaybackStore`, `useLibraryStore`, `useUiStore` — jamais de tick 60/120 Hz dans Zustand) + **TanStack Virtual** (`@tanstack/react-virtual`) pour toutes les tracklists et la file d'attente.
   - **Routing :** **React Router** (ou routing léger typé synchronisé avec un historique `Alt+←/→`).

### 5.2 Contrat IPC & Typage bout en bout

| Flux | Canal Tauri v2 | Fréquence | Typage |
| --- | --- | --- | --- |
| Bootstrap initial (`UiSnapshot`, préférences, devices WASAPI, session restaurée) | `invoke("get_bootstrap_state")` | 1× au démarrage | `tauri-specta` |
| Commandes lecture, file d'attente, favoris, playlists, recherche, réglages WASAPI/Last.fm | `#[tauri::command] async fn` | À la demande | `tauri-specta` (`Result<T, AppError>`) |
| Événements discrets (`TrackChanged` + palette 4 couleurs, `PlaybackStateChanged`, `FormatNegotiated`, `TrackTransitioned`, `DevicesUpdated`, `LibrarySynced`, `Smtc/Tray`) | Événements `tauri-specta` (`app.emit`) | < 5 Hz | `tauri-specta` |
| Télémétrie temps réel (`[f32; 5]` audio + `position_ms` + `duration_ms` + `buffered_ms`) | `Channel<InvokeResponseBody::Raw>` (`ArrayBuffer` fixe 44 octets) | 60 Hz actif / 0 Hz au repos | Layout binaire documenté (`DataView`) |
| Pochettes albums/artistes/playlists | Protocole custom `tidal-cover://` | À la demande (caché Chromium + disque) | Binaire `image/jpeg` |

> **Note sur les entiers 64 bits (`u64`) :** les IDs TIDAL et durées en millisecondes tiennent largement dans les 53 bits sûrs de JS (`Number.MAX_SAFE_INTEGER`). Configurer `specta` avec export `number` sûr (ou `string` pour tout identifiant externe non garanti < 2^53) afin d'éviter la friction `bigint` dans React.

### 5.3 Système de design visuel (« Sobre & Premium »)

**Décision prise :** s'aligner sur la vraie identité visuelle actuelle de [app/src/theme.rs](file:///C:/Users/KEDI-ADMIN/Documents/Project/tidenative/app/src/theme.rs) (**Spatial Immersion / TIDAL OLED Dark**) et **ne pas** utiliser l'ancien thème indigo/vert de `design-system/tidenative/tokens.json`.

- **Surfaces :** fond pur OLED `#040406`, ambient top `#0A0B0F`, cartes `#111116`, surfaces flottantes verre sombre `rgba(18, 18, 24, 0.86)` avec filet spéculaire `1 px` (`rgba(255, 255, 255, 0.08` → `0.13`).
- **Typographie :** police sans-serif système propre et lisible (`Inter` / `Segoe UI Variable`), chiffres tabulaires (`tabular-nums`) obligatoires sur tous les timers, durées, débits (`24-bit / 192 kHz`) et numéros de pistes pour zéro tremblement horizontal.
- **Badges officiels TIDAL (exigence `AGENTS.md`) :** `MAX` `#FFD432`, `HIGH` `#33F3FF`, `LOW` `#A8A8B3`, indicateur `Bit-Perfect` `#30D158`.
- **Agencement spatial :**
  - Barre de titre frameless discrète intégrée en haut avec la **pilule de navigation flottante** (`Écouter · Explorer · Collection · Playlists · Activité · Réglages`), bouton recherche (`Ctrl+K`) et contrôles fenêtre natifs Windows 11.
  - Zone principale aérée, grilles de pochettes réelles systématiques, listes virtualisées propres avec actions contextuelles au survol.
  - **Îlot de lecture flottant** en bas (`PlayerBar`) regroupant pochette, métadonnées + badge qualité/DAC, contrôles + scrubber haute précision, volume et déclencheurs `MatrixOrb` / `QueuePanel` / `NowPlaying`.
  - **Fond dynamique sobre :** extraction de 3–4 couleurs harmonisées par pochette en Rust (luminosité plafonnée pour préserver un contraste WCAG AA/AAA), projetées en halo supérieur subtil sur les vues principales et en dégradé immersif doux dans `NowPlaying`.

### 5.4 Garde-fous 120 Hz & UX Desktop Windows 11

1. **Zéro `setState` par frame :**
   - Scrubber piloté par `MotionValue` (`scaleX`), temps `mm:ss` isolé dans un composant feuille mis à jour à 1 Hz (au changement de seconde).
   - `MatrixOrb` (`@rare-ui/matrix-orb` ou canvas 2D dédié) lit directement un `Float32Array(5)` hors React à 120 Hz dans `requestAnimationFrame` en lissant les paquets binaires reçus à 60 Hz.
2. **Propriétés GPU uniquement :**
   - Uniquement `transform` et `opacity` sur toutes les animations (jamais `transition: all`, jamais de `width`/`margin` animé sur l'ouverture du `QueuePanel`, jamais de variable CSS héritée animée sur un parent de liste, jamais de `blur()` > 20 px animé).
   - Animations pendant chargement/navigation (indicateur d'onglet, transitions de page) exécutées via **CSS / WAAPI** (`motion` >= 14.2) pour rester fluides sur le compositor même pendant le montage d'une nouvelle vue.
   - Combinaison **shadcn/Radix + Motion** via `forceMount` + `asChild` + `AnimatePresence` pour préserver focus trap, pile `Escape` et animations de sortie propres.
3. **Expérience native Windows 11 :**
   - Fenêtre créée `visible: false` + `backgroundColor: "#040406"` + CSS inline `#040406` dans `index.html` + `apply_windows11_styling` avant `window.show()` après le premier paint React → **zéro flash blanc**.
   - Frameless avec `data-tauri-drag-region`, gestion `WM_NCHITTEST` (ou `tauri-plugin-decorum`) pour préserver coins arrondis, ombre portée, resize fluide et **Snap Layouts** Windows 11.
   - Désactivation globale de `contextmenu` natif navigateur, du zoom Ctrl+molette, du drag d'images et de `user-select` hors champs de saisie / paroles / crédits.
   - Interception `CloseRequested` → réduction tray si `minimize_to_tray` actif ; filtre clavier global ignorant les éléments `input, textarea, [contenteditable]`.

---

## 6. Feuille de route incrémentale & Vérification

- **Phase 1 — Extraction `crates/app-engine` :** isoler toute la logique métier/orchestration d'`app/src/main.rs` dans `crates/app-engine` avec tests unitaires de la file d'attente, génération de lecture et filtres.
- **Phase 2 — Fondation Tauri v2 + PoC 120 Hz :** créer `app-tauri/` + `ui/` (Vite, React, Tailwind v4, shadcn/ui, Motion, `@rare-ui/matrix-orb`, `tauri-specta`), valider la compilation sur `stable-x86_64-pc-windows-gnu`, la fenêtre frameless zéro-flash + DWM/SMTC/Tray, `tidal-cover://` + extraction palette, et mesurer **120 Hz (< 8,33 ms/frame)** sur `MatrixOrb` + fond ambiant + liste virtualisée de 2 000 pistes dans DevTools WebView2.
- **Phase 3 — Cœur d'écoute (MVP) :** `TopNav`, `PlayerBar` (scrubber 0 re-render, volume, badges MAX/HIGH/LOW + Bit-Perfect), login WebView2 intégré, `Discover` et `Favorites` virtualisés.
- **Phase 4 — Catalogue & Navigation complète :** `Explore` + recherche instantanée (FTS5 + API TIDAL + `Command` `Ctrl+K`), `AlbumDetail`, `ArtistDetail`, `Playlists` / dossiers / CRUD, `Collection`, menus contextuels et raccourcis clavier complets.
- **Phase 5 — Immersion, Activité & Réglages Audiophiles :** `NowPlaying` (fond dynamique plein écran, `MatrixOrb`, paroles synchronisées, crédits), `QueuePanel` (réordonnancement drag-and-drop), `Activity` + Last.fm, `AudioSettings` (WASAPI exclusif, bit-perfect lock, ReplayGain, hot-plug DAC).
- **Phase 6 — Audit 120 Hz, Packaging & Nettoyage :** profilage Performance WebView2 sur scénarios réels, mise à jour de `installer/Build-Release-Dist.ps1`, puis suppression de l'ancien dossier `app/`.

---

## 7. Choix explicitement laissés à l'implémentation (Builder)

- Choix exact de la bibliothèque de routing client (React Router v7 en mode SPA ou TanStack Router).
- Algorithme précis d'extraction de palette côté Rust (ex. quantification légère k-means / median-cut sur 32×32 pixels à partir du JPEG décodé dans `CoverCache`, sans alourdir le temps de compilation).
- Découpage fin des fichiers de commandes Tauri dans `app-tauri/src/commands/` et des composants React dans `ui/src/`.
- Mise à jour de `AGENTS.md` à la fin de la migration pour refléter la nouvelle architecture Tauri v2 + React.
