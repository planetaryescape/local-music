import { Cover } from '@/implementation/renderer/components/cover'
import type { Album } from '@/implementation/renderer/lib/albums'

/** Opens an album on click and plays it from the overlay button. */
export function AlbumCard(props: {
  album: Album
  onOpen: (album: Album) => void
  onPlay: (album: Album) => void
}) {
  const playable = () => props.album.tracks.some((track) => track.presence === 'present')

  return (
    <article class="album-card">
      <button class="album-open" type="button" onClick={() => props.onOpen(props.album)}>
        <span class="album-art">
          <Cover
            trackId={props.album.cover?.id ?? null}
            artworkId={props.album.cover?.artworkId ?? null}
            size="large"
          />
        </span>
        <strong>{props.album.title}</strong>
        <span>{props.album.artist}</span>
        <small>{props.album.tracks.length} tracks</small>
      </button>
      <button
        class="album-play"
        type="button"
        aria-label={`Play ${props.album.title}`}
        disabled={!playable()}
        onClick={() => props.onPlay(props.album)}
      >
        ▶
      </button>
    </article>
  )
}
