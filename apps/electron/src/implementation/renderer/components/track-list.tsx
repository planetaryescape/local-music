import { For, Show } from 'solid-js'

import type { LibraryTrack } from '@/contracts/library'
import { Cover } from '@/implementation/renderer/components/cover'
import { formatTime } from '@/implementation/renderer/lib/time'

/** Renders the searchable track table and exposes play intent to the owning player. */
export function TrackList(props: {
  tracks: ReadonlyArray<LibraryTrack>
  layout: 'library' | 'album'
  currentTrackId: string | null
  playing: boolean
  onPlay: (track: LibraryTrack) => void
}) {
  return (
    <div
      class="track-table min-h-0 flex-1 overflow-auto"
      classList={{ 'is-album': props.layout === 'album' }}
      role="list"
      aria-label="Tracks"
    >
      <div class="track-head" aria-hidden="true">
        <span>#</span>
        <span>Title</span>
        <span>Album</span>
        <span>Time</span>
      </div>
      <Show when={props.tracks.length === 0}>
        <div class="empty-state">No tracks match this view.</div>
      </Show>
      <For each={props.tracks.slice(0, 500)}>
        {(track, index) => (
          <div
            class="track-row"
            classList={{
              'is-current': props.currentTrackId === track.id,
              'is-missing': track.presence === 'missing',
            }}
            role="listitem"
            onDblClick={() => track.presence === 'present' && props.onPlay(track)}
          >
            <button
              class="track-index"
              type="button"
              aria-label={`Play ${track.title}`}
              disabled={track.presence === 'missing'}
              onClick={() => props.onPlay(track)}
            >
              <span class="track-number">{String(index() + 1).padStart(2, '0')}</span>
              <span class="track-play" aria-hidden="true">
                {props.currentTrackId === track.id && props.playing ? 'Ⅱ' : '▶'}
              </span>
            </button>
            <Cover trackId={track.id} artworkId={track.artworkId} size="small" />
            <div class="track-title">
              <strong>{track.title}</strong>
              <span>
                {track.artist}
                {track.presence === 'missing' ? ' · File missing' : ''}
              </span>
            </div>
            <span class="track-album">{track.album}</span>
            <time class="track-time">{formatTime(track.durationSeconds)}</time>
          </div>
        )}
      </For>
    </div>
  )
}
