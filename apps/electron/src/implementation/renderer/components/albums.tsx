import { For } from 'solid-js'

import { AlbumCard } from '@/implementation/renderer/components/album-card'
import type { Album } from '@/implementation/renderer/lib/albums'

/** Shows a short shelf of albums on the library home. */
export function Albums(props: {
  albums: ReadonlyArray<Album>
  onOpen: (album: Album) => void
  onPlay: (album: Album) => void
  onShowAll: () => void
}) {
  return (
    <section class="album-section" aria-labelledby="albums-title">
      <div class="section-heading">
        <h2 id="albums-title">Albums</h2>
        <button class="text-button" type="button" onClick={props.onShowAll}>
          All {props.albums.length} albums →
        </button>
      </div>
      <div class="album-shelf">
        <For each={props.albums.slice(0, 6)}>
          {(album) => <AlbumCard album={album} onOpen={props.onOpen} onPlay={props.onPlay} />}
        </For>
      </div>
    </section>
  )
}
