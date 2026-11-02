import assert from 'node:assert/strict'
import { mkdir, mkdtemp, realpath, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, test } from 'node:test'

import { Effect, Stream } from 'effect'

import { LibraryFailure, SourceScanner, type ScannedPath } from '@/contracts/library'
import { makeArtworkCache } from '@/implementation/main/artwork-cache'
import { serveArtwork } from '@/implementation/main/artwork-protocol'
import { serveAudio } from '@/implementation/main/audio-protocol'
import { openLibraryDatabase } from '@/implementation/main/db/open'
import { makeLibraryStore } from '@/implementation/main/db/store'
import { makeLibrary } from '@/implementation/main/library'

const directories: string[] = []

const migrations = join(process.cwd(), 'drizzle')

afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true })
})

test('retains saved metadata and cached artwork when a later traversal fails or a file goes missing', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'local-music-library-'))
  directories.push(directory)
  const sourcePath = join(directory, 'music')
  await mkdir(sourcePath)
  const canonicalSourcePath = await realpath(sourcePath)
  const database = await openLibraryDatabase(join(directory, 'library.db'), migrations)
  const store = makeLibraryStore(database.db)
  const cache = await Effect.runPromise(makeArtworkCache(join(directory, 'artwork')))
  const cover = { mimeType: 'image/png', bytes: new Uint8Array([1, 2, 3]) }

  const song: ScannedPath = {
    kind: 'observed',
    observation: {
      path: 'song.mp3',
      title: 'Song',
      artist: 'Artist',
      album: 'Album',
      durationSeconds: 10,
      cover,
    },
  }

  let paths: ScannedPath[] = [song]
  let fail = false

  const scanner = SourceScanner.of({
    scan: () =>
      fail
        ? Stream.fail(new LibraryFailure({ message: 'Traversal incomplete' }))
        : Stream.fromIterable(paths),
  })

  const library = await Effect.runPromise(makeLibrary(store, scanner, cache))
  const first = await Effect.runPromise(library.addFolder(sourcePath))
  const id = first.tracks[0]?.id

  assert.ok(id)
  assert.equal(
    await Effect.runPromise(library.audioPath(id)),
    join(canonicalSourcePath, 'song.mp3'),
  )
  assert.deepEqual((await Effect.runPromise(library.artwork(id))).bytes, cover.bytes)

  let requestedAudioPath = ''

  const audio = await serveAudio(
    library,
    new Request(`local-music-audio://track/${id}`, {
      headers: { Range: 'bytes=4-8' },
    }),
    (path, headers) => {
      requestedAudioPath = path

      return Promise.resolve(
        new Response('audio', {
          status: 206,
          headers: { 'Requested-Range': headers.get('Range') ?? '' },
        }),
      )
    },
  )

  assert.equal(audio.status, 206)
  assert.equal(audio.headers.get('Requested-Range'), 'bytes=4-8')
  assert.equal(requestedAudioPath, join(canonicalSourcePath, 'song.mp3'))

  const rejectedAudio = await serveAudio(
    library,
    new Request(`local-music-audio://track/${'0'.repeat(8)}-0000-0000-0000-${'0'.repeat(12)}`),
    () => Promise.reject(new Error('Fetcher must not receive an unknown track')),
  )

  assert.equal(rejectedAudio.status, 404)

  const artworkId = first.tracks[0]?.artworkId

  assert.ok(artworkId)

  const response = await serveArtwork(
    library,
    new Request(`local-music-artwork://cover/${id}/${artworkId}`),
  )

  assert.equal(response.status, 200)
  assert.equal(response.headers.get('Content-Type'), cover.mimeType)
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), cover.bytes)

  const rejected = await serveArtwork(
    library,
    new Request(`local-music-artwork://cover/${id}/${'0'.repeat(64)}`),
  )

  assert.equal(rejected.status, 404)

  fail = true
  await assert.rejects(Effect.runPromise(library.rescan(first.sources[0]?.id ?? '')))
  assert.deepEqual(await Effect.runPromise(library.load()), first)

  fail = false
  paths = []
  const missing = await Effect.runPromise(library.rescan(first.sources[0]?.id ?? ''))
  assert.equal(missing.tracks[0]?.id, id)
  assert.equal(missing.tracks[0]?.presence, 'missing')
  await assert.rejects(Effect.runPromise(library.audioPath(id)))
  assert.deepEqual((await Effect.runPromise(library.artwork(id))).bytes, cover.bytes)

  database.close()
})
