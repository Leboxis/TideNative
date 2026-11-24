use std::collections::HashMap;
use std::time::{Duration, Instant};
use tidal_core::{RepeatMode, SessionPlaybackState, TrackSummary};

pub const PLAY_HISTORY_LIMIT: usize = 50;
pub const SESSION_QUEUE_LIMIT: usize = 200;
pub const SEEK_HOLD_DURATION: Duration = Duration::from_millis(650);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackStep {
    Play(usize),
    Restart,
    Stop,
    Ignore,
}

#[derive(Debug, Clone)]
pub struct QueueState {
    pub queue: Vec<TrackSummary>,
    pub original_queue: Vec<TrackSummary>,
    pub queue_index: Option<usize>,
    pub current_track: Option<TrackSummary>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub buffered_ms: u64,
    pub play_history: Vec<TrackSummary>,
    pub playback_generation: u64,
    pub seek_hold_until: Option<Instant>,
    pub pending_resume_seek_ms: Option<u64>,
    pub radio_requested_for_track: Option<u64>,
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            original_queue: Vec::new(),
            queue_index: None,
            current_track: None,
            position_ms: 0,
            duration_ms: 0,
            buffered_ms: 0,
            play_history: Vec::new(),
            playback_generation: 0,
            seek_hold_until: None,
            pending_resume_seek_ms: None,
            radio_requested_for_track: None,
        }
    }
}

impl QueueState {
    pub fn bump_generation(&mut self) -> u64 {
        self.playback_generation = self.playback_generation.wrapping_add(1);
        self.playback_generation
    }

    pub fn generation(&self) -> u64 {
        self.playback_generation
    }

    pub fn is_current_generation(&self, generation: u64) -> bool {
        self.playback_generation == generation
    }

    pub fn accept_radio_tracks(&mut self, generation: u64, tracks: Vec<TrackSummary>) -> bool {
        if !self.is_current_generation(generation) || tracks.is_empty() {
            return false;
        }
        let existing: std::collections::HashSet<u64> =
            self.queue.iter().map(|t| t.id).collect();
        let fresh: Vec<TrackSummary> = tracks
            .into_iter()
            .filter(|t| !existing.contains(&t.id))
            .collect();
        if fresh.is_empty() {
            return false;
        }
        queue_append_many(&mut self.queue, &mut self.original_queue, &fresh);
        true
    }

    pub fn begin_seek(&mut self, target_ms: u64, now: Instant) {
        self.position_ms = if self.duration_ms > 0 {
            target_ms.min(self.duration_ms)
        } else {
            target_ms
        };
        self.seek_hold_until = Some(now + SEEK_HOLD_DURATION);
    }

    pub fn apply_player_position(&mut self, pos_ms: u64, dur_ms: u64, now: Instant) -> bool {
        if dur_ms > 0 {
            self.duration_ms = dur_ms;
        }
        if let Some(until) = self.seek_hold_until {
            if now < until {
                return false;
            }
            self.seek_hold_until = None;
        }
        self.position_ms = pos_ms;
        true
    }

    pub fn set_queue_and_select(
        &mut self,
        tracks: Vec<TrackSummary>,
        start_index: usize,
        shuffle_enabled: bool,
    ) -> Option<TrackSummary> {
        if tracks.is_empty() {
            return None;
        }
        let clamped_idx = start_index.min(tracks.len() - 1);
        self.original_queue = tracks.clone();
        if shuffle_enabled {
            let (shuffled, new_idx) =
                shuffle_queue_keeping_current(&tracks, Some(clamped_idx), shuffle_seed());
            self.queue = shuffled;
            self.queue_index = new_idx;
        } else {
            self.queue = tracks;
            self.queue_index = Some(clamped_idx);
        }
        self.select_index(self.queue_index?)
    }

    pub fn select_index(&mut self, idx: usize) -> Option<TrackSummary> {
        let track = self.queue.get(idx)?.clone();
        self.bump_generation();
        self.queue_index = Some(idx);
        self.current_track = Some(track.clone());
        self.position_ms = 0;
        self.duration_ms = u64::from(track.duration_secs) * 1000;
        self.buffered_ms = 0;
        self.seek_hold_until = None;
        self.pending_resume_seek_ms = None;
        self.radio_requested_for_track = None;
        record_play_history(&mut self.play_history, &track);
        Some(track)
    }

    pub fn toggle_shuffle(&mut self, enabled: bool) {
        let current_id = self.current_track.as_ref().map(|t| t.id);
        let queue = std::mem::take(&mut self.queue);
        let original = std::mem::take(&mut self.original_queue);
        let (new_q, new_orig, new_idx) = toggle_shuffle_state(
            queue,
            original,
            self.queue_index,
            current_id,
            enabled,
            shuffle_seed(),
        );
        self.queue = new_q;
        self.original_queue = new_orig;
        self.queue_index = new_idx;
    }

    pub fn insert_next(&mut self, track: TrackSummary) -> usize {
        queue_insert_next(
            &mut self.queue,
            &mut self.original_queue,
            self.queue_index,
            track,
        )
    }

    pub fn append(&mut self, track: TrackSummary) {
        queue_append(&mut self.queue, &mut self.original_queue, track);
    }

    pub fn remove_at(&mut self, idx: usize) -> bool {
        queue_remove_at(
            &mut self.queue,
            &mut self.original_queue,
            &mut self.queue_index,
            idx,
        )
    }

    pub fn move_item(&mut self, from_idx: usize, to_idx: usize) -> bool {
        let min_upcoming = self.queue_index.map(|c| c + 1).unwrap_or(0);
        if from_idx < min_upcoming
            || to_idx < min_upcoming
            || from_idx >= self.queue.len()
            || to_idx >= self.queue.len()
            || from_idx == to_idx
        {
            return false;
        }
        let item = self.queue.remove(from_idx);
        self.queue.insert(to_idx, item);
        true
    }

    pub fn clear_upcoming(&mut self) -> usize {
        queue_clear_upcoming(&mut self.queue, &mut self.original_queue, self.queue_index)
    }

    pub fn gapless_next_track_id(&self, repeat: RepeatMode) -> Option<u64> {
        gapless_next_track_id(&self.queue, self.queue_index, repeat)
    }

    pub fn export_session(&self) -> Option<SessionPlaybackState> {
        if self.queue.is_empty() {
            return None;
        }
        let (windowed_queue, windowed_idx) = session_queue_window(&self.queue, self.queue_index);
        let (windowed_orig, _) = session_queue_window(&self.original_queue, None);
        Some(SessionPlaybackState {
            queue: windowed_queue,
            original_queue: windowed_orig,
            queue_index: windowed_idx,
            position_ms: self.position_ms,
        })
    }

    pub fn restore_from_session(&mut self, session: &SessionPlaybackState) -> bool {
        let Some((queue, original, index, current, pos_ms, dur_ms)) =
            restore_session_state(session)
        else {
            return false;
        };
        self.queue = queue;
        self.original_queue = original;
        self.queue_index = index;
        self.current_track = current;
        self.position_ms = pos_ms;
        self.duration_ms = dur_ms;
        self.pending_resume_seek_ms = if pos_ms > 0 { Some(pos_ms) } else { None };
        true
    }
}

pub fn shuffle_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        ^ (u64::from(std::process::id()) << 32)
}

pub fn shuffle_queue_keeping_current(
    queue: &[TrackSummary],
    current_idx: Option<usize>,
    seed: u64,
) -> (Vec<TrackSummary>, Option<usize>) {
    let mut state = if seed == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        seed
    };
    let mut next_rand = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    match current_idx.filter(|idx| *idx < queue.len()) {
        Some(idx) => {
            let mut tail: Vec<TrackSummary> = queue
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != idx)
                .map(|(_, track)| track.clone())
                .collect();
            fisher_yates(&mut tail, &mut next_rand);

            let mut shuffled = Vec::with_capacity(queue.len());
            shuffled.push(queue[idx].clone());
            shuffled.extend(tail);
            (shuffled, Some(0))
        }
        None => {
            let mut shuffled = queue.to_vec();
            fisher_yates(&mut shuffled, &mut next_rand);
            (shuffled, None)
        }
    }
}

pub fn unshuffle_queue(
    original: &[TrackSummary],
    current_track_id: Option<u64>,
) -> (Vec<TrackSummary>, Option<usize>) {
    let idx = current_track_id.and_then(|id| original.iter().position(|t| t.id == id));
    (original.to_vec(), idx)
}

pub fn toggle_shuffle_state(
    mut queue: Vec<TrackSummary>,
    mut original_queue: Vec<TrackSummary>,
    mut queue_index: Option<usize>,
    current_track_id: Option<u64>,
    shuffle_enabled: bool,
    seed: u64,
) -> (Vec<TrackSummary>, Vec<TrackSummary>, Option<usize>) {
    if shuffle_enabled {
        if !queue.is_empty() {
            original_queue = queue.clone();
            let (shuffled, new_idx) = shuffle_queue_keeping_current(&queue, queue_index, seed);
            queue = shuffled;
            if let Some(idx) = new_idx {
                queue_index = Some(idx);
            }
        }
    } else if !original_queue.is_empty() {
        let (restored, idx) = unshuffle_queue(&original_queue, current_track_id);
        queue = restored;
        queue_index = idx;
    }
    (queue, original_queue, queue_index)
}

fn fisher_yates(tracks: &mut [TrackSummary], next_rand: &mut impl FnMut() -> u64) {
    if tracks.len() < 2 {
        return;
    }
    for i in (1..tracks.len()).rev() {
        let j = (next_rand() % (i as u64 + 1)) as usize;
        tracks.swap(i, j);
    }
}

pub fn next_track_step(queue_index: Option<usize>, queue_len: usize, repeat: RepeatMode) -> TrackStep {
    if queue_len == 0 {
        return TrackStep::Ignore;
    }
    match queue_index {
        Some(idx) if idx + 1 < queue_len => TrackStep::Play(idx + 1),
        Some(_) if repeat == RepeatMode::All => TrackStep::Play(0),
        Some(_) => TrackStep::Stop,
        None => TrackStep::Play(0),
    }
}

pub fn track_finished_step(
    queue_index: Option<usize>,
    queue_len: usize,
    repeat: RepeatMode,
) -> TrackStep {
    if repeat == RepeatMode::One {
        return TrackStep::Restart;
    }
    next_track_step(queue_index, queue_len, repeat)
}

pub fn previous_track_step(
    position_ms: u64,
    queue_index: Option<usize>,
    queue_len: usize,
    repeat: RepeatMode,
) -> TrackStep {
    if queue_len == 0 {
        return TrackStep::Ignore;
    }
    let Some(idx) = queue_index.filter(|idx| *idx < queue_len) else {
        return TrackStep::Ignore;
    };
    if position_ms > 4_000 {
        return TrackStep::Restart;
    }
    if idx > 0 {
        return TrackStep::Play(idx - 1);
    }
    if repeat == RepeatMode::All && queue_len > 1 {
        return TrackStep::Play(queue_len - 1);
    }
    TrackStep::Restart
}

pub fn gapless_next_track_id(
    queue: &[TrackSummary],
    queue_index: Option<usize>,
    repeat: RepeatMode,
) -> Option<u64> {
    let idx = queue_index.filter(|idx| *idx < queue.len())?;
    match repeat {
        RepeatMode::One => queue.get(idx).map(|t| t.id),
        RepeatMode::All => {
            if queue.is_empty() {
                None
            } else {
                queue.get((idx + 1) % queue.len()).map(|t| t.id)
            }
        }
        RepeatMode::Off => queue.get(idx + 1).map(|t| t.id),
    }
}

pub fn next_track_index_for_transition(
    queue: &[TrackSummary],
    queue_index: Option<usize>,
    repeat_mode: RepeatMode,
    to_track_id: u64,
) -> Option<usize> {
    let Some(idx) = queue_index.filter(|idx| *idx < queue.len()) else {
        return queue.iter().position(|track| track.id == to_track_id);
    };
    let expected = match repeat_mode {
        RepeatMode::One => return Some(idx),
        RepeatMode::All => (idx + 1) % queue.len(),
        RepeatMode::Off => idx + 1,
    };
    if queue
        .get(expected)
        .is_some_and(|track| track.id == to_track_id)
    {
        return Some(expected);
    }
    queue.iter().position(|track| track.id == to_track_id)
}

pub fn session_queue_window(
    queue: &[TrackSummary],
    queue_index: Option<usize>,
) -> (Vec<TrackSummary>, Option<usize>) {
    if queue.len() <= SESSION_QUEUE_LIMIT {
        return (
            queue.to_vec(),
            queue_index.filter(|idx| *idx < queue.len()),
        );
    }
    let Some(idx) = queue_index.filter(|idx| *idx < queue.len()) else {
        return (queue[..SESSION_QUEUE_LIMIT].to_vec(), None);
    };
    let start = idx
        .saturating_sub(SESSION_QUEUE_LIMIT / 2)
        .min(queue.len() - SESSION_QUEUE_LIMIT);
    (
        queue[start..start + SESSION_QUEUE_LIMIT].to_vec(),
        Some(idx - start),
    )
}

pub fn restore_session_state(
    session: &SessionPlaybackState,
) -> Option<(
    Vec<TrackSummary>,
    Vec<TrackSummary>,
    Option<usize>,
    Option<TrackSummary>,
    u64,
    u64,
)> {
    if session.queue.is_empty() {
        return None;
    }
    let queue = session.queue.clone();
    let original_queue = if session.original_queue.is_empty() {
        queue.clone()
    } else {
        session.original_queue.clone()
    };
    let queue_index = session.queue_index.filter(|idx| *idx < queue.len());
    let current_track = queue_index.and_then(|idx| queue.get(idx)).cloned();
    let duration_ms = current_track
        .as_ref()
        .map(|track| u64::from(track.duration_secs) * 1000)
        .unwrap_or(0);
    let position_ms = session.position_ms.min(duration_ms);
    Some((
        queue,
        original_queue,
        queue_index,
        current_track,
        position_ms,
        duration_ms,
    ))
}

pub fn queue_insert_next(
    queue: &mut Vec<TrackSummary>,
    original: &mut Vec<TrackSummary>,
    queue_index: Option<usize>,
    track: TrackSummary,
) -> usize {
    let insert_pos = queue_index
        .map(|idx| (idx + 1).min(queue.len()))
        .unwrap_or(0);
    queue.insert(insert_pos, track.clone());
    original.insert(insert_pos.min(original.len()), track);
    insert_pos
}

pub fn queue_append(
    queue: &mut Vec<TrackSummary>,
    original: &mut Vec<TrackSummary>,
    track: TrackSummary,
) {
    queue.push(track.clone());
    original.push(track);
}

pub fn queue_append_many(
    queue: &mut Vec<TrackSummary>,
    original: &mut Vec<TrackSummary>,
    tracks: &[TrackSummary],
) {
    queue.extend_from_slice(tracks);
    original.extend_from_slice(tracks);
}

fn sync_original_to_queue(original: &mut Vec<TrackSummary>, queue: &[TrackSummary]) {
    let mut needed: HashMap<u64, usize> = HashMap::new();
    for track in queue {
        *needed.entry(track.id).or_default() += 1;
    }
    let mut kept = Vec::with_capacity(original.len());
    for track in original.iter() {
        if let Some(count) = needed.get_mut(&track.id) {
            if *count > 0 {
                *count -= 1;
                kept.push(track.clone());
            }
        }
    }
    *original = kept;
}

pub fn queue_remove_at(
    queue: &mut Vec<TrackSummary>,
    original: &mut Vec<TrackSummary>,
    queue_index: &mut Option<usize>,
    idx: usize,
) -> bool {
    if idx >= queue.len() || *queue_index == Some(idx) {
        return false;
    }
    queue.remove(idx);
    if let Some(cur) = *queue_index {
        if idx < cur {
            *queue_index = Some(cur - 1);
        }
    }
    sync_original_to_queue(original, queue);
    true
}

pub fn queue_clear_upcoming(
    queue: &mut Vec<TrackSummary>,
    original: &mut Vec<TrackSummary>,
    queue_index: Option<usize>,
) -> usize {
    match queue_index {
        Some(cur) => {
            let start = (cur + 1).min(queue.len());
            let removed = queue.drain(start..).count();
            sync_original_to_queue(original, queue);
            removed
        }
        None => {
            let removed = queue.len();
            queue.clear();
            original.clear();
            removed
        }
    }
}

pub fn record_play_history(history: &mut Vec<TrackSummary>, track: &TrackSummary) {
    history.retain(|entry| entry.id != track.id);
    history.insert(0, track.clone());
    history.truncate(PLAY_HISTORY_LIMIT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_track(id: u64) -> TrackSummary {
        TrackSummary {
            id,
            title: format!("Track {id}"),
            artist_name: "Artist".into(),
            artist_id: Some(1),
            album_id: Some(1),
            album_title: "Album".into(),
            cover_id: None,
            duration_secs: 180,
            audio_quality: Some("LOSSLESS".into()),
            explicit: false,
        }
    }

    #[test]
    fn stale_playback_generation_discards_late_radio_response() {
        let mut q = QueueState::default();
        let gen1 = q.bump_generation();
        let _gen2 = q.bump_generation();
        assert!(!q.accept_radio_tracks(gen1, vec![dummy_track(99)]));
        assert!(q.queue.is_empty());
    }

    #[test]
    fn current_playback_generation_accepts_and_deduplicates_radio_tracks() {
        let mut q = QueueState::default();
        q.set_queue_and_select(vec![dummy_track(1), dummy_track(2)], 0, false);
        let generation = q.generation();
        assert!(q.accept_radio_tracks(generation, vec![dummy_track(2), dummy_track(3)]));
        let ids: Vec<u64> = q.queue.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn seek_hold_window_ignores_stale_position_updates_for_650ms() {
        let mut q = QueueState::default();
        q.duration_ms = 180_000;
        let now = Instant::now();
        q.begin_seek(45_000, now);
        assert!(!q.apply_player_position(12_000, 180_000, now + Duration::from_millis(300)));
        assert_eq!(q.position_ms, 45_000);
        assert!(q.apply_player_position(45_700, 180_000, now + Duration::from_millis(700)));
        assert_eq!(q.position_ms, 45_700);
    }

    #[test]
    fn shuffle_and_unshuffle_preserve_current_track_and_original_order() {
        let mut q = QueueState::default();
        let tracks: Vec<TrackSummary> = (1..=6).map(dummy_track).collect();
        q.set_queue_and_select(tracks, 3, false);
        assert_eq!(q.current_track.as_ref().map(|t| t.id), Some(4));

        q.toggle_shuffle(true);
        assert_eq!(q.queue_index, Some(0));
        assert_eq!(q.queue[0].id, 4);

        q.toggle_shuffle(false);
        assert_eq!(q.queue_index, Some(3));
        assert_eq!(
            q.queue.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn session_restore_clamps_position_and_sets_pending_resume_seek() {
        let mut q = QueueState::default();
        let session = SessionPlaybackState {
            queue: vec![dummy_track(1), dummy_track(2)],
            original_queue: Vec::new(),
            queue_index: Some(1),
            position_ms: 999_999,
        };
        assert!(q.restore_from_session(&session));
        assert_eq!(q.queue_index, Some(1));
        assert_eq!(q.position_ms, 180_000);
        assert_eq!(q.pending_resume_seek_ms, Some(180_000));
    }
}
