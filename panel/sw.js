/* Minimal service worker: keeps Ryoku Box installable as a fullscreen PWA.
   Network-first, no offline staleness for the agent's live data. */
self.addEventListener('install', (event) => self.skipWaiting());
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', () => { /* network only: live agent data */ });
