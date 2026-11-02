import { Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js'

import type { LibraryTrack } from '@/contracts/library'
import { Cover } from '@/implementation/renderer/components/cover'
import { QueueStrip } from '@/implementation/renderer/components/queue-strip'
import { formatTime } from '@/implementation/renderer/lib/time'

/** Plays indexed tracks and renders the persistent transport controls. */
export function Player(props: {
  track: LibraryTrack | null
  context: string
  queue: ReadonlyArray<LibraryTrack>
  queueIndex: number
  onSelect: (index: number) => void
  hasPrevious: boolean
  hasNext: boolean
  onPrevious: () => void
  onNext: () => void
  onPlayingChange: (playing: boolean) => void
  onError: (message: string) => void
}) {
  let audio: HTMLAudioElement | undefined
  const [playing, setPlaying] = createSignal(false)
  const [position, setPosition] = createSignal(0)
  const [duration, setDuration] = createSignal(0)

  createEffect(() => {
    const track = props.track

    if (!audio || !track) return

    audio.src = `local-music-audio://track/${track.id}`
    audio.load()
    setPosition(0)
    setDuration(track.durationSeconds)
    audio.play().catch((error) => {
      if (error instanceof DOMException && error.name === 'AbortError') return
      props.onError(`Could not play “${track.title}”.`)
    })
  })

  const setPlayback = (next: boolean) => {
    setPlaying(next)
    props.onPlayingChange(next)
  }

  const toggle = () => {
    if (!audio || !props.track) return

    if (audio.paused) void audio.play()
    else audio.pause()
  }

  onMount(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== ' ') return

      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLButtonElement)
        return

      event.preventDefault()
      toggle()
    }

    document.addEventListener('keydown', onKeyDown)
    onCleanup(() => document.removeEventListener('keydown', onKeyDown))
  })

  const seek = (value: string) => {
    if (!audio) return
    const next = Number(value)
    audio.currentTime = next
    setPosition(next)
  }

  return (
    <footer class="player" aria-label="Now playing">
      <audio
        ref={(element) => {
          audio = element
        }}
        onPlay={() => setPlayback(true)}
        onPause={() => setPlayback(false)}
        onTimeUpdate={(event) => setPosition(event.currentTarget.currentTime)}
        onDurationChange={(event) => setDuration(event.currentTarget.duration)}
        onEnded={props.onNext}
        onError={() => {
          setPlayback(false)

          if (props.track) props.onError(`Could not play “${props.track.title}”.`)
        }}
      />
      <Show
        when={props.track}
        fallback={
          <div class="player-empty">
            <span class="player-placeholder" aria-hidden="true">
              ♪
            </span>
            <span>
              <strong>Nothing playing</strong>
              <small>Choose a track to begin</small>
            </span>
          </div>
        }
      >
        {(track) => (
          <>
            <QueueStrip
              tracks={props.queue}
              index={props.queueIndex}
              progress={position() / Math.max(duration(), 1)}
              onSelect={props.onSelect}
            />
            <div class="now-playing">
              <Cover trackId={track().id} artworkId={track().artworkId} size="small" />
              <span>
                <strong>{track().title}</strong>
                <small>
                  {track().artist} · {track().album}
                </small>
              </span>
            </div>
            <div class="transport">
              <div class="transport-buttons">
                <button
                  type="button"
                  aria-label="Previous track"
                  disabled={!props.hasPrevious}
                  onClick={props.onPrevious}
                >
                  ◀
                </button>
                <button
                  class="play-toggle"
                  type="button"
                  onClick={toggle}
                  aria-label="Play or pause"
                  title="Play or pause (Space)"
                >
                  {playing() ? 'Ⅱ' : '▶'}
                </button>
                <button
                  type="button"
                  aria-label="Next track"
                  disabled={!props.hasNext}
                  onClick={props.onNext}
                >
                  ▶
                </button>
              </div>
              <div class="timeline">
                <time>{formatTime(position())}</time>
                <input
                  type="range"
                  min="0"
                  max={Math.max(duration(), 1)}
                  step="0.1"
                  value={position()}
                  aria-label="Playback position"
                  style={{ '--progress': `${(position() / Math.max(duration(), 1)) * 100}%` }}
                  onInput={(event) => seek(event.currentTarget.value)}
                />
                <time>{formatTime(duration())}</time>
              </div>
            </div>
            <div class="player-context">
              <span>Playing from</span>
              <strong>{props.context}</strong>
            </div>
          </>
        )}
      </Show>
    </footer>
  )
}
