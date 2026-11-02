import { For, Show } from 'solid-js'

import { AlbumCard } from '@/implementation/renderer/components/album-card'
import type { Album } from '@/implementation/renderer/lib/albums'

/** Lists every album in the library as a browsable grid. */
export function AlbumGrid(props: {
  albums: ReadonlyArray<Album>
  onOpen: (album: Album) => void
  onPlay: (album: Album) => void
}) {
  return (
    <section class="album-grid" aria-label="Albums">
      <Show when={props.albums.length === 0}>
        <div class="empty-state">No albums yet. Add a folder to begin.</div>
      </Show>
      <For each={props.albums}>
        {(album) => <AlbumCard album={album} onOpen={props.onOpen} onPlay={props.onPlay} />}
      </For>
    </section>
  )
}
