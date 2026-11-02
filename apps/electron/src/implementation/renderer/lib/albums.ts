import type { LibraryTrack } from '@/contracts/library'

export interface Album {
  readonly key: string
  readonly title: string
  readonly artist: string
  readonly tracks: ReadonlyArray<LibraryTrack>
  readonly cover: LibraryTrack | null
  readonly durationSeconds: number
}

export const albumKey = (track: LibraryTrack) => `${track.artist}\0${track.album}`

/** Groups tracks into albums, ordering each album by file path since tags carry no track numbers yet. */
export function groupAlbums(tracks: ReadonlyArray<LibraryTrack>): ReadonlyArray<Album> {
  const found = new Map<string, LibraryTrack[]>()

  for (const track of tracks) {
    const key = albumKey(track)
    const group = found.get(key)

    if (group) group.push(track)
    else found.set(key, [track])
  }

  return [...found].map(([key, group]) => {
    const sorted = [...group].sort((a, b) =>
      a.path.localeCompare(b.path, undefined, { numeric: true }),
    )

    return {
      key,
      title: sorted[0]?.album ?? '',
      artist: sorted[0]?.artist ?? '',
      tracks: sorted,
      cover: sorted.find((track) => track.artworkId) ?? null,
      durationSeconds: sorted.reduce((total, track) => total + track.durationSeconds, 0),
    }
  })
}

export function albumHue(track: LibraryTrack): number {
  let hash = 0

  for (const char of albumKey(track)) hash = (Math.imul(hash, 31) + char.charCodeAt(0)) | 0

  return Math.round(Math.abs(hash) * 137.508) % 360
}
