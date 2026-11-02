import { Show, createEffect, createSignal, on } from 'solid-js'

export function Cover(props: {
  trackId: string | null
  artworkId: string | null
  size: 'small' | 'large'
}) {
  const size = () => (props.size === 'small' ? 'size-[44px]' : 'size-full')
  const [failed, setFailed] = createSignal(false)

  createEffect(
    on(
      () => [props.trackId, props.artworkId],
      () => setFailed(false),
    ),
  )

  return (
    <Show
      when={props.trackId && props.artworkId && !failed()}
      fallback={
        <div
          class={`grid shrink-0 place-items-center rounded-md bg-soft ${size()}`}
          aria-label="No album artwork"
        >
          <span class="record" />
        </div>
      }
    >
      <img
        class={`shrink-0 rounded-md object-cover ${size()}`}
        src={`local-music-artwork://cover/${props.trackId}/${props.artworkId}`}
        alt=""
        loading="lazy"
        decoding="async"
        onError={() => setFailed(true)}
      />
    </Show>
  )
}
