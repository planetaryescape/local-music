export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds)) return '0:00'

  return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`
}

export function formatRuntime(seconds: number): string {
  const minutes = Math.round(seconds / 60)

  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} hr ${minutes % 60} min`
}
