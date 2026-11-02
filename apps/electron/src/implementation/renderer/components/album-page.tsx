import { Show } from 'solid-js'

import type { LibraryTrack } from '@/contracts/library'
import { Cover } from '@/implementation/renderer/components/cover'
import { TrackList } from '@/implementation/renderer/components/track-list'
import type { Album } from '@/implementation/renderer/lib/albums'
import { formatRuntime } from '@/implementation/renderer/lib/time'

/** Shows one album with its tracks in file order. */
export function AlbumPage(props: {
  album: Album
  currentTrackId: string | null
  playing: boolean
  onBack: () => void
  onPlay: (album: Album) => void
  onPlayTrack: (track: LibraryTrack) => void
}) {
  const missing = () => props.album.tracks.filter((track) => track.presence === 'missing').length

  return (
    <section class="album-page" aria-labelledby="album-title">
      <button class="text-button" type="button" onClick={props.onBack}>
        ← Back <kbd>esc</kbd>
      </button>
      <header class="album-hero">
        <span class="album-hero-art">
          <Cover
            trackId={props.album.cover?.id ?? null}
            artworkId={props.album.cover?.artworkId ?? null}
            size="large"
          />
        </span>
        <div class="album-hero-text">
          <span class="eyebrow">Album</span>
          <h2 id="album-title">{props.album.title}</h2>
          <p>{props.album.artist}</p>
          <dl class="album-facts">
            <div>
              <dt>Tracks</dt>
              <dd>{props.album.tracks.length}</dd>
            </div>
            <div>
              <dt>Length</dt>
              <dd>{formatRuntime(props.album.durationSeconds)}</dd>
            </div>
            <Show when={missing() > 0}>
              <div>
                <dt>Missing</dt>
                <dd>{missing()}</dd>
              </div>
            </Show>
          </dl>
          <button
            class="primary-button"
            type="button"
            disabled={missing() === props.album.tracks.length}
            onClick={() => props.onPlay(props.album)}
          >
            ▶ Play album
          </button>
        </div>
      </header>
      <TrackList
        tracks={props.album.tracks}
        layout="album"
        currentTrackId={props.currentTrackId}
        playing={props.playing}
        onPlay={props.onPlayTrack}
      />
    </section>
  )
}
