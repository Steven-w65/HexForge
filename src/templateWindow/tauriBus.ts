import { emitTo, listen } from '@tauri-apps/api/event'
import type { LocalBus } from './protocol'

// Only local Tauri events are used. Nothing is transmitted over a network.
export const tauriBus: LocalBus = {
  listen: (event, callback) => listen(event, ({ payload }) => { void callback(payload) }),
  send: (target, event, payload) => emitTo(target, event, payload),
}
