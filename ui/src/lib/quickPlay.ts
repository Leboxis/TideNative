import { toast } from "sonner";
import { type TrackSummary, commands } from "@/bindings";
import { telemetryRuntime } from "@/lib/telemetry";
import { usePlaybackStore } from "@/stores/playbackStore";

export async function quickPlayTracks(tracks: TrackSummary[], sequential = false) {
  if (tracks.length === 0) {
    toast.info("Aucun titre disponible à lire.");
    return;
  }
  if (sequential && usePlaybackStore.getState().preferences.shuffle_enabled) {
    await commands.setShuffle(false);
  }
  telemetryRuntime.seedPosition(0, tracks[0].duration_secs * 1000);
  const res = await commands.playTracks(tracks, 0);
  if (res.status === "error") {
    toast.error(res.error);
  }
}

export async function quickPlayAlbum(albumId: number) {
  const res = await commands.getAlbum(albumId);
  if (res.status === "error") {
    toast.error(res.error);
    return;
  }
  await quickPlayTracks(res.data.tracks, true);
}

export async function quickPlayPlaylist(playlistUuid: string) {
  const res = await commands.getPlaylistTracks(playlistUuid);
  if (res.status === "error") {
    toast.error(res.error);
    return;
  }
  await quickPlayTracks(res.data.tracks, false);
}

export async function quickPlayMix(mixId: string, existingTracks?: TrackSummary[]) {
  if (existingTracks && existingTracks.length > 0) {
    await quickPlayTracks(existingTracks, false);
    return;
  }
  const res = await commands.getMixTracks(mixId);
  if (res.status === "error") {
    toast.error(res.error);
    return;
  }
  await quickPlayTracks(res.data, false);
}
