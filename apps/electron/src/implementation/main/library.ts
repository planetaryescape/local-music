import { createHash, randomUUID } from 'node:crypto'
import { realpath } from 'node:fs/promises'

import { Effect, Layer, Semaphore, Stream } from 'effect'

import {
  ArtworkCache,
  Library,
  LibraryFailure,
  LibraryStore,
  SourceScanner,
} from '@/contracts/library'

export const makeLibrary = Effect.fn('Library.make')(function* (
  store: typeof LibraryStore.Service,
  scanner: typeof SourceScanner.Service,
  cache: typeof ArtworkCache.Service,
) {
  const scans = yield* Semaphore.make(1)

  const load = store.load

  const rescan = (sourceId: string) =>
    scans.withPermits(1)(
      Effect.gen(function* () {
        const snapshot = yield* store.load()
        const source = snapshot.sources.find((item) => item.id === sourceId)

        if (!source) return yield* new LibraryFailure({ message: 'Music folder not found.' })

        const scanId = randomUUID()
        yield* cache.prune(yield* store.referencedArtwork())
        yield* store.begin(source.id, scanId)

        const scan = scanner.scan(source.rootPath).pipe(
          Stream.runForEach((item) =>
            Effect.gen(function* () {
              const cover = item.kind === 'observed' ? item.observation.cover : null
              const artworkId = cover ? yield* cache.store(cover) : null
              yield* store.stage(
                source.id,
                scanId,
                item,
                artworkId,
                artworkId ? (cover?.mimeType ?? null) : null,
              )
            }),
          ),
        )

        const updated = yield* scan.pipe(
          Effect.flatMap(() => store.commit(source.id, scanId, Date.now())),
          Effect.ensuring(store.discard(source.id, scanId).pipe(Effect.ignore)),
        )

        yield* cache.prune(yield* store.referencedArtwork())

        return updated
      }),
    )

  const addFolder = Effect.fn('Library.addFolder')(function* (root: string) {
    const canonical = yield* Effect.tryPromise({
      try: () => realpath(root),
      catch: () => new LibraryFailure({ message: 'Could not access this music folder.' }),
    })

    const source = yield* store.register(canonical)

    return yield* rescan(source.id)
  })

  const artwork = Effect.fn('Library.artwork')(function* (trackId: string) {
    const track = yield* store.findTrack(trackId)

    if (!track?.artworkId || !track.artworkMimeType) {
      return yield* new LibraryFailure({ message: 'Album artwork is not available.' })
    }

    const bytes = yield* cache.read(track.artworkId)

    if (
      createHash('sha256').update(track.artworkMimeType).update(bytes).digest('hex') !==
      track.artworkId
    ) {
      return yield* new LibraryFailure({ message: 'Album artwork is not available.' })
    }

    return { mimeType: track.artworkMimeType, bytes }
  })

  const audioPath = Effect.fn('Library.audioPath')(function* (trackId: string) {
    const track = yield* store.findTrack(trackId)

    if (!track || track.presence !== 'present') {
      return yield* new LibraryFailure({ message: 'The audio file is not available.' })
    }

    return track.path
  })

  return Library.of({ load, addFolder, rescan, audioPath, artwork })
})

export const libraryLayer = Layer.effect(
  Library,
  Effect.gen(function* () {
    return yield* makeLibrary(yield* LibraryStore, yield* SourceScanner, yield* ArtworkCache)
  }),
)
