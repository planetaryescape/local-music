import { For, Show, createMemo } from 'solid-js'

import type { LibraryTrack } from '@/contracts/library'
import { albumHue } from '@/implementation/renderer/lib/albums'

const behind = 8

const ahead = 40

/** Draws the queue around the current track as blocks sized by duration and tinted by album. */
export function QueueStrip(props: {
  tracks: ReadonlyArray<LibraryTrack>
  index: number
  progress: number
  onSelect: (index: number) => void
}) {
  const start = () => Math.max(0, props.index - behind)
  const visible = createMemo(() => props.tracks.slice(start(), props.index + ahead + 1))
  const remaining = () => Math.max(0, props.tracks.length - (props.index + ahead + 1))

  return (
    <div class="queue-strip" aria-label="Queue">
      <For each={visible()}>
        {(track, offset) => {
          const index = () => start() + offset()

          return (
            <button
              type="button"
              class="queue-block"
              classList={{
                'is-played': index() < props.index,
                'is-current': index() === props.index,
              }}
              style={{
                'flex-grow': Math.max(track.durationSeconds, 30),
                '--hue': albumHue(track),
                '--fill': index() === props.index ? `${props.progress * 100}%` : '0%',
              }}
              title={`${track.title} · ${track.artist}`}
              aria-label={`Play ${track.title}`}
              aria-current={index() === props.index}
              onClick={() => props.onSelect(index())}
            />
          )
        }}
      </For>
      <Show when={remaining() > 0}>
        <span class="queue-more">+{remaining()}</span>
      </Show>
    </div>
  )
}
