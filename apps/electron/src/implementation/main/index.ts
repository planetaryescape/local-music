import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

import { Effect, Layer, Schema } from 'effect'
import { app, BrowserWindow, dialog, ipcMain, net, protocol } from 'electron'

import { Library, type LibraryReply, type LibrarySnapshot } from '@/contracts/library'
import { artworkCacheLayer } from '@/implementation/main/artwork-cache'
import { serveArtwork } from '@/implementation/main/artwork-protocol'
import { serveAudio } from '@/implementation/main/audio-protocol'
import { openLibraryDatabase } from '@/implementation/main/db/open'
import { libraryStoreLayer } from '@/implementation/main/db/store'
import { libraryLayer } from '@/implementation/main/library'
import { sourceScannerLayer } from '@/implementation/main/scanner'
import { stopTelemetry, traceLibraryOperation } from '@/implementation/main/telemetry'

function createWindow() {
  const output = join(app.getAppPath(), 'dist')

  const window = new BrowserWindow({
    width: 1320,
    height: 860,
    minWidth: 780,
    minHeight: 540,
    title: 'local-music',
    backgroundColor: '#191e1d',
    webPreferences: {
      preload: join(output, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  })

  void window.loadFile(join(output, 'index.html'))
}

app.setName('local-music')

protocol.registerSchemesAsPrivileged([
  { scheme: 'local-music-artwork', privileges: { standard: true, secure: true } },
  {
    scheme: 'local-music-audio',
    privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true },
  },
])

void app.whenReady().then(async () => {
  if (!app.isPackaged && process.env.LOCAL_MUSIC_TEST_USER_DATA) {
    app.setPath('userData', process.env.LOCAL_MUSIC_TEST_USER_DATA)
  }

  const userData = app.getPath('userData')

  const database = await openLibraryDatabase(
    join(userData, 'library.db'),
    join(app.getAppPath(), 'dist', 'drizzle'),
  )

  const adapters = Layer.mergeAll(
    libraryStoreLayer(database.db),
    sourceScannerLayer,
    artworkCacheLayer(join(userData, 'artwork')),
  )

  const library = await Effect.runPromise(
    Library.pipe(Effect.provide(libraryLayer.pipe(Layer.provide(adapters)))),
  )

  protocol.handle('local-music-artwork', (request) => serveArtwork(library, request))
  protocol.handle('local-music-audio', (request) =>
    serveAudio(library, request, (path, headers) =>
      net.fetch(pathToFileURL(path).toString(), { headers }),
    ),
  )

  const notifyLibrary = (snapshot: LibrarySnapshot) => {
    for (const window of BrowserWindow.getAllWindows())
      window.webContents.send('library:changed', snapshot)
  }

  const notifyScan = (state: { sourceId: string; running: boolean; error?: string }) => {
    for (const window of BrowserWindow.getAllWindows())
      window.webContents.send('library:scan-state', state)
  }

  const rescan = async (sourceId: string): Promise<LibraryReply> => {
    try {
      notifyScan({ sourceId, running: true })

      const data = await traceLibraryOperation('library.rescan', () =>
        Effect.runPromise(library.rescan(sourceId)),
      )

      notifyLibrary(data)

      return { ok: true, data }
    } catch {
      notifyScan({
        sourceId,
        running: false,
        error: 'Could not fully scan this folder.',
      })

      return { ok: false, message: 'Could not fully scan this folder. Saved tracks were kept.' }
    } finally {
      notifyScan({ sourceId, running: false })
    }
  }

  ipcMain.handle('library:load', async (): Promise<LibraryReply> => {
    try {
      const data = await traceLibraryOperation('library.load', () =>
        Effect.runPromise(library.load()),
      )

      return { ok: true, data }
    } catch {
      return { ok: false, message: 'Could not open the saved library.' }
    }
  })

  ipcMain.handle('library:choose-folder', async (event): Promise<LibraryReply> => {
    const window = BrowserWindow.fromWebContents(event.sender)

    if (!window) return { ok: false, message: 'The window has closed.' }

    const testFolder = !app.isPackaged ? process.env.LOCAL_MUSIC_TEST_FOLDER : undefined
    let folder = testFolder

    if (!folder) {
      const selection = await dialog.showOpenDialog(window, {
        properties: ['openDirectory'],
        title: 'Choose a music folder',
      })

      folder = selection.filePaths[0]

      if (selection.canceled || !folder) return { ok: false, cancelled: true }
    }

    try {
      const data = await traceLibraryOperation('library.addFolder', () =>
        Effect.runPromise(library.addFolder(folder)),
      )

      notifyLibrary(data)

      return { ok: true, data }
    } catch {
      return { ok: false, message: 'Could not scan this folder. Saved tracks were kept.' }
    }
  })

  ipcMain.handle('library:rescan', async (_event, raw): Promise<LibraryReply> => {
    try {
      const sourceId = Schema.decodeUnknownSync(Schema.NonEmptyString)(raw)

      return await rescan(sourceId)
    } catch {
      return { ok: false, message: 'Invalid music folder.' }
    }
  })

  let telemetryStopped = false
  app.on('before-quit', (event) => {
    if (telemetryStopped) return
    event.preventDefault()
    telemetryStopped = true
    void stopTelemetry()
      .catch(() => {
        // Export failure must not prevent the app from closing.
      })
      .finally(() => {
        database.close()
        app.quit()
      })
  })
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})
