# TideNative — Project Instructions & Official TIDAL Reference

## Architecture actuelle (Tauri v2 + React 19 à 120 Hz + Cœur Rust natif)

- **Workspace Rust (édition 2024, `stable-x86_64-pc-windows-gnu`)** :
  - `crates/core`, `crates/tidal-auth`, `crates/tidal-stream`, `crates/decoder`, `crates/audio-wasapi`, `crates/player`, `crates/tidal-api`, `crates/cache`, `crates/platform-win` : cœur métier, WASAPI Exclusif Bit-Perfect, cache SQLite FTS5 + pochettes + extraction de palette (`CoverPalette`), DWM/SMTC/Tray Windows.
  - `crates/app-engine` : orchestrateur headless (`AppEngine`) indépendant de l'UI (file d'attente, `playback_generation`, fenêtre anti-jitter `seek_hold_until` 650 ms, filtres explicite/IA, Last.fm, pont SMTC/Tray, démarrage à froid `< 10 ms`).
  - `app-tauri` (`tidenative-tauri`, binaire `TideNative`) : shell desktop Windows Tauri v2 (fenêtre frameless `visible: false` `#040406` zéro flash blanc, protocole asynchrone `tidal-cover://`, `Channel<InvokeResponseBody::Raw>` binaire 44 octets à 60 Hz pausé au repos, commandes/événements typés `tauri-specta` -> `ui/src/bindings.ts`).
- **Frontend 120 Hz (`ui/`)** :
  - Vite 6, React 19, TypeScript, Tailwind CSS v4 (palette OLED Spatial Immersion `#040406` / `#111116`), primitives shadcn/ui (Radix UI), Motion (`motion/react`), Zustand, TanStack Virtual, Sonner, Vitest.
  - Règles 120 Hz : zéro `setState` par frame (`Scrubber` via `MotionValue` `scaleX`, `MatrixOrbCanvas` lisant `Float32Array(5)` dans `requestAnimationFrame`), fond ambiant réactif à la pochette en crossfade `opacity` GPU uniquement.

## Documentation officielle TIDAL (référence permanente)

Pour tout travail sur l'API, l'authentification, le streaming, les vues **Écouter** / **Explorer** ou le modèle de données, se référer systématiquement à la documentation officielle TIDAL :

- **Portail développeur & guides** : https://developer.tidal.com/documentation
- **Référence OpenAPI v2 (`THIRD_PARTY`)** : https://tidal-music.github.io/tidal-api-reference/?accessTier=THIRD_PARTY
  - Spécification OpenAPI JSON : `https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json`
  - Serveur de base : `https://openapi.tidal.com/v2`
  - Format : JSON:API (`Accept: application/vnd.api+json`), ressources avec `attributes`, `relationships` et `included` via `include=...`
- **TIDAL SDK for Web** : https://tidal-music.github.io/tidal-sdk-web/

## Endpoints officiels `THIRD_PARTY` pertinents (`https://openapi.tidal.com/v2`)

### Onglet Écouter (accueil personnalisé)
- `GET /users/me` — profil & pays de l'utilisateur connecté
- `GET /userDailyMixes/{id}?include=items` — mix quotidiens personnalisés (`My Daily Discovery` / `My Mix`)
- `GET /userDiscoveryMixes/{id}?include=items` — mix découverte personnalisé
- `GET /userNewReleaseMixes/{id}?include=items` — sorties récentes personnalisées (`My New Arrivals`)
- `GET /userCollectionTracks/{id}/relationships/items?include=items` — favoris pistes
- `GET /userCollectionAlbums/{id}/relationships/items?include=items` — albums sauvegardés
- `GET /userCollectionArtists/{id}/relationships/items?include=items` — artistes suivis
- `GET /userCollectionPlaylists/{id}/relationships/items?include=items` — playlists utilisateur

### Onglet Explorer (découverte, recherche & catalogue)
- `GET /searchResults?filter[query]={q}&include=albums,artists,playlists,topHits,tracks` — recherche catalogue complète
- `GET /searchResults/{id}/relationships/{topHits,albums,artists,playlists,tracks}` — résultats ciblés par catégorie
- `GET /searchSuggestions?filter[query]={q}&include=directHits` — autocomplétion instantanée
- `GET /tracks/{id}/relationships/similarTracks` & `GET /tracks/{id}/relationships/radio` — pistes similaires & radio
- `GET /albums/{id}/relationships/similarAlbums`, `/albums/{id}/relationships/coverArt`, `/albums/{id}/relationships/items`
- `GET /artists/{id}/relationships/similarArtists`, `/artists/{id}/relationships/radio`, `/artists/{id}/relationships/albums`, `/artists/{id}/relationships/profileArt`
- `GET /playlists/{id}/relationships/items`, `/playlists/{id}/relationships/coverArt`
- `GET /artworks/{id}` — miniatures haute résolution (pochettes, bannières, visuels)

### Lecture & Manifests
- `GET /trackManifests/{id}?manifestType=MPEG_DASH&formats=FLAC_HIRES,FLAC,AACLC&uriScheme=HTTPS&usage=PLAYBACK&adaptive=false` (OpenAPI v2)
- Compatibilité session Web / PKCE v1 : `GET https://api.tidal.com/v1/tracks/{id}/playbackinfo` (ou `playbackinfopostpaywall`)

## Conventions UI & Performance

- Style visuel natif TIDAL : fonds sombres profonds (`#040406`), miniatures/pochettes réelles systématiques via `tidal-cover://`, badges qualité officiels (`MAX` `#FFD432`, `HIGH` `#33F3FF`, `LOW` `#A8A8B3`, `Bit-Perfect` `#30D158`).
- Lecture & seek à faible latence : préchargement parallèle (`init.mp4` + 1er segment en parallèle), cache des segments DASH et des `PlaybackInfo`, requêtes HTTP `Range` à la demande pour le seek sur les flux BTS/Direct, annulation immédiate des chargements obsolètes.
- Packaging Windows : `powershell -NoProfile -File installer/Build-Release-Dist.ps1` (exécute `pnpm --dir ui build`, `cargo build --release -p tidenative-tauri --bin TideNative`, et empaquette `TideNative.exe` + `WebView2Loader.dll`).
