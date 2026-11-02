import { randomUUID } from 'node:crypto'
import { join } from 'node:path'

import { and, eq, notInArray } from 'drizzle-orm'
import { Effect, Layer, Schema } from 'effect'

import {
  LibraryFailure,
  LibrarySnapshot,
  LibraryStore,
  LibraryTrack,
  type ScannedPath,
} from '@/contracts/library'
import type { LibraryDatabase } from '@/implementation/main/db/open'
import { librarySources, scanStagePaths, tracks } from '@/implementation/main/db/schema'

const storageFailure = () => new LibraryFailure({ message: 'Could not read or save the library.' })

const trackRow = (track: typeof tracks.$inferSelect, rootPath: string) => ({
  id: track.id,
  sourceId: track.sourceId,
  relativePath: track.relativePath,
  path: join(rootPath, track.relativePath),
  title: track.observedTitle,
  artist: track.observedArtist,
  album: track.observedAlbum,
  durationSeconds: track.durationSeconds,
  hasEmbeddedArtwork: track.hasEmbeddedArtwork,
  artworkId: track.artworkId,
  artworkMimeType: track.artworkMimeType,
  presence: track.presence,
  lastSeenAt: track.lastSeenAt,
})

export function makeLibraryStore(db: LibraryDatabase) {
  const load = Effect.fn('LibraryStore.load')(function* () {
    const rows = yield* Effect.tryPromise({
      try: () => Promise.all([db.select().from(librarySources), db.select().from(tracks)]),
      catch: storageFailure,
    })

    const [sources, savedTracks] = rows
    const roots = new Map(sources.map((source) => [source.id, source.rootPath]))

    const snapshot = {
      sources,
      tracks: savedTracks.map((track) => trackRow(track, roots.get(track.sourceId) ?? '')),
    }

    return yield* Schema.decodeUnknownEffect(LibrarySnapshot)(snapshot).pipe(
      Effect.mapError(storageFailure),
    )
  })

  const register = Effect.fn('LibraryStore.register')(function* (root: string) {
    const source = yield* Effect.tryPromise({
      try: async () => {
        await db
          .insert(librarySources)
          .values({
            id: randomUUID(),
            rootPath: root,
            addedAt: Date.now(),
            lastSuccessfulScanAt: null,
          })
          .onConflictDoNothing()
          .run()

        const [row] = await db
          .select()
          .from(librarySources)
          .where(eq(librarySources.rootPath, root))

        return row
      },
      catch: storageFailure,
    })

    if (!source) return yield* storageFailure()

    return source
  })

  const begin = Effect.fn('LibraryStore.begin')(function* (sourceId: string, scanId: string) {
    yield* Effect.tryPromise({
      try: () =>
        db
          .delete(scanStagePaths)
          .where(and(eq(scanStagePaths.sourceId, sourceId), eq(scanStagePaths.scanId, scanId)))
          .run(),
      catch: storageFailure,
    })
  })

  const stage = Effect.fn('LibraryStore.stage')(function* (
    sourceId: string,
    scanId: string,
    item: ScannedPath,
    artworkId: string | null,
    artworkMimeType: string | null,
  ) {
    const values =
      item.kind === 'observed'
        ? {
            scanId,
            sourceId,
            relativePath: item.observation.path,
            readState: 'observed' as const,
            observedTitle: item.observation.title,
            observedArtist: item.observation.artist,
            observedAlbum: item.observation.album,
            durationSeconds: item.observation.durationSeconds,
            hasEmbeddedArtwork: item.observation.cover !== null,
            artworkId,
            artworkMimeType,
          }
        : {
            scanId,
            sourceId,
            relativePath: item.path,
            readState: 'unreadable' as const,
            observedTitle: null,
            observedArtist: null,
            observedAlbum: null,
            durationSeconds: null,
            hasEmbeddedArtwork: null,
            artworkId: null,
            artworkMimeType: null,
          }

    yield* Effect.tryPromise({
      try: () => db.insert(scanStagePaths).values(values).run(),
      catch: storageFailure,
    })
  })

  const discard = Effect.fn('LibraryStore.discard')(function* (sourceId: string, scanId: string) {
    yield* Effect.tryPromise({
      try: () =>
        db
          .delete(scanStagePaths)
          .where(and(eq(scanStagePaths.sourceId, sourceId), eq(scanStagePaths.scanId, scanId)))
          .run(),
      catch: storageFailure,
    })
  })

  const commit = Effect.fn('LibraryStore.commit')(function* (
    sourceId: string,
    scanId: string,
    completedAt: number,
  ) {
    yield* Effect.tryPromise({
      try: () =>
        db.transaction(async (tx) => {
          const stageFilter = and(
            eq(scanStagePaths.sourceId, sourceId),
            eq(scanStagePaths.scanId, scanId),
          )

          const seen = tx
            .select({ relativePath: scanStagePaths.relativePath })
            .from(scanStagePaths)
            .where(stageFilter)

          await tx
            .update(tracks)
            .set({ presence: 'missing' })
            .where(and(eq(tracks.sourceId, sourceId), notInArray(tracks.relativePath, seen)))
            .run()

          for (let offset = 0; ; offset += 200) {
            const rows = await tx
              .select()
              .from(scanStagePaths)
              .where(stageFilter)
              .limit(200)
              .offset(offset)

            if (rows.length === 0) break

            for (const row of rows) {
              if (row.readState === 'unreadable') {
                await tx
                  .update(tracks)
                  .set({ presence: 'present', lastSeenAt: completedAt })
                  .where(
                    and(eq(tracks.sourceId, sourceId), eq(tracks.relativePath, row.relativePath)),
                  )
                  .run()
                continue
              }

              if (
                row.observedTitle === null ||
                row.observedArtist === null ||
                row.observedAlbum === null ||
                row.durationSeconds === null ||
                row.hasEmbeddedArtwork === null
              )
                throw storageFailure()

              const observed = {
                observedTitle: row.observedTitle,
                observedArtist: row.observedArtist,
                observedAlbum: row.observedAlbum,
                durationSeconds: row.durationSeconds,
                hasEmbeddedArtwork: row.hasEmbeddedArtwork,
                artworkId: row.artworkId,
                artworkMimeType: row.artworkMimeType,
                presence: 'present' as const,
                lastSeenAt: completedAt,
              }

              await tx
                .insert(tracks)
                .values({
                  id: randomUUID(),
                  sourceId,
                  relativePath: row.relativePath,
                  ...observed,
                })
                .onConflictDoUpdate({
                  target: [tracks.sourceId, tracks.relativePath],
                  set: observed,
                })
                .run()
            }
          }

          await tx
            .update(librarySources)
            .set({ lastSuccessfulScanAt: completedAt })
            .where(eq(librarySources.id, sourceId))
            .run()
          await tx.delete(scanStagePaths).where(stageFilter).run()
        }),
      catch: storageFailure,
    })

    return yield* load()
  })

  const referencedArtwork = Effect.fn('LibraryStore.referencedArtwork')(function* () {
    const rows = yield* Effect.tryPromise({
      try: () => db.select({ id: tracks.artworkId }).from(tracks),
      catch: storageFailure,
    })

    return new Set(rows.flatMap((row) => (row.id === null ? [] : [row.id])))
  })

  const findTrack = Effect.fn('LibraryStore.findTrack')(function* (id: string) {
    const [row] = yield* Effect.tryPromise({
      try: () =>
        db
          .select({ track: tracks, rootPath: librarySources.rootPath })
          .from(tracks)
          .innerJoin(librarySources, eq(librarySources.id, tracks.sourceId))
          .where(eq(tracks.id, id))
          .limit(1),
      catch: storageFailure,
    })

    if (!row) return null

    return yield* Schema.decodeUnknownEffect(LibraryTrack)(trackRow(row.track, row.rootPath)).pipe(
      Effect.mapError(storageFailure),
    )
  })

  return LibraryStore.of({
    load,
    register,
    begin,
    stage,
    commit,
    discard,
    referencedArtwork,
    findTrack,
  })
}

export const libraryStoreLayer = (db: LibraryDatabase) =>
  Layer.succeed(LibraryStore, makeLibraryStore(db))
