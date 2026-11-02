import { Context, Schema } from 'effect'
import type { Effect, Stream } from 'effect'

export const LibrarySource = Schema.Struct({
  id: Schema.NonEmptyString,
  rootPath: Schema.String,
  addedAt: Schema.Number,
  lastSuccessfulScanAt: Schema.NullOr(Schema.Number),
})

export interface LibrarySource extends Schema.Schema.Type<typeof LibrarySource> {}

export const LibraryTrack = Schema.Struct({
  id: Schema.NonEmptyString,
  sourceId: Schema.NonEmptyString,
  relativePath: Schema.NonEmptyString,
  path: Schema.String,
  title: Schema.String,
  artist: Schema.String,
  album: Schema.String,
  durationSeconds: Schema.Number,
  hasEmbeddedArtwork: Schema.Boolean,
  artworkId: Schema.NullOr(Schema.String),
  artworkMimeType: Schema.NullOr(Schema.String),
  presence: Schema.Literals(['present', 'missing']),
  lastSeenAt: Schema.Number,
})

export interface LibraryTrack extends Schema.Schema.Type<typeof LibraryTrack> {}

export const LibrarySnapshot = Schema.Struct({
  sources: Schema.Array(LibrarySource),
  tracks: Schema.Array(LibraryTrack),
})

export interface LibrarySnapshot extends Schema.Schema.Type<typeof LibrarySnapshot> {}

export const LibraryReply = Schema.Union([
  Schema.Struct({ ok: Schema.Literal(true), data: LibrarySnapshot }),
  Schema.Struct({ ok: Schema.Literal(false), message: Schema.String }),
  Schema.Struct({ ok: Schema.Literal(false), cancelled: Schema.Literal(true) }),
])

export type LibraryReply = typeof LibraryReply.Type

export class LibraryFailure extends Schema.TaggedError<LibraryFailure>()('LibraryFailure', {
  message: Schema.String,
}) {}

export interface ScannedObservation {
  readonly path: string
  readonly title: string
  readonly artist: string
  readonly album: string
  readonly durationSeconds: number
  readonly cover: { readonly mimeType: string; readonly bytes: Uint8Array } | null
}

export type ScannedPath =
  | { readonly kind: 'observed'; readonly observation: ScannedObservation }
  | { readonly kind: 'unreadable'; readonly path: string }

export class SourceScanner extends Context.Service<
  SourceScanner,
  {
    readonly scan: (root: string) => Stream.Stream<ScannedPath, LibraryFailure>
  }
>()('@local-music/SourceScanner') {}

export class LibraryStore extends Context.Service<
  LibraryStore,
  {
    readonly load: () => Effect.Effect<LibrarySnapshot, LibraryFailure>
    readonly register: (root: string) => Effect.Effect<LibrarySource, LibraryFailure>
    readonly begin: (sourceId: string, scanId: string) => Effect.Effect<void, LibraryFailure>
    readonly stage: (
      sourceId: string,
      scanId: string,
      path: ScannedPath,
      artworkId: string | null,
      artworkMimeType: string | null,
    ) => Effect.Effect<void, LibraryFailure>
    readonly commit: (
      sourceId: string,
      scanId: string,
      completedAt: number,
    ) => Effect.Effect<LibrarySnapshot, LibraryFailure>
    readonly discard: (sourceId: string, scanId: string) => Effect.Effect<void, LibraryFailure>
    readonly referencedArtwork: () => Effect.Effect<ReadonlySet<string>, LibraryFailure>
    readonly findTrack: (id: string) => Effect.Effect<LibraryTrack | null, LibraryFailure>
  }
>()('@local-music/LibraryStore') {}

export class ArtworkCache extends Context.Service<
  ArtworkCache,
  {
    readonly store: (cover: {
      readonly mimeType: string
      readonly bytes: Uint8Array
    }) => Effect.Effect<string | null, LibraryFailure>
    readonly read: (id: string) => Effect.Effect<Uint8Array, LibraryFailure>
    readonly prune: (referenced: ReadonlySet<string>) => Effect.Effect<void, LibraryFailure>
  }
>()('@local-music/ArtworkCache') {}

export class Library extends Context.Service<
  Library,
  {
    readonly load: () => Effect.Effect<LibrarySnapshot, LibraryFailure>
    readonly addFolder: (root: string) => Effect.Effect<LibrarySnapshot, LibraryFailure>
    readonly rescan: (sourceId: string) => Effect.Effect<LibrarySnapshot, LibraryFailure>
    readonly audioPath: (trackId: string) => Effect.Effect<string, LibraryFailure>
    readonly artwork: (
      trackId: string,
    ) => Effect.Effect<{ readonly mimeType: string; readonly bytes: Uint8Array }, LibraryFailure>
  }
>()('@local-music/Library') {}
