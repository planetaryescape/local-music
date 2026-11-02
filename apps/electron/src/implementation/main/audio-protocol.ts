import { Effect } from 'effect'

import type { Library } from '@/contracts/library'

const audioPath = /^\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/

/** Serves an indexed audio file without exposing arbitrary filesystem paths to the renderer. */
export async function serveAudio(
  library: typeof Library.Service,
  request: Request,
  fetchFile: (path: string, headers: Headers) => Promise<Response>,
): Promise<Response> {
  const url = new URL(request.url)
  const match = audioPath.exec(url.pathname)

  if (
    request.method !== 'GET' ||
    url.protocol !== 'local-music-audio:' ||
    url.hostname !== 'track' ||
    url.port !== '' ||
    url.username !== '' ||
    url.password !== '' ||
    url.search !== '' ||
    url.hash !== '' ||
    !match
  )
    return new Response(null, { status: 404 })

  const [, trackId] = match

  try {
    const path = await Effect.runPromise(library.audioPath(trackId))

    return await fetchFile(path, request.headers)
  } catch {
    return new Response(null, { status: 404 })
  }
}
