// Mkrew web push (ADR 0066). Every push shows a notification, even when the page is open:
// iOS removes a subscription that receives pushes without showing one.
self.addEventListener('push', (event) => {
  let note = {};
  try {
    note = event.data ? event.data.json() : {};
  } catch {
    // Not JSON: show the generic text below.
  }
  event.waitUntil(
    self.registration.showNotification(note.title || 'Mkrew', {
      body: note.body || 'An agent needs you.',
      // A newer notice for the same session replaces the older one.
      tag: note.session || 'mkrew',
      icon: '/apple-touch-icon.png',
      data: { session: note.session || null },
    }),
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const session = event.notification.data?.session;
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
      const open = windows[0];
      if (open) {
        try {
          await open.focus();
        } catch {
          // Some browsers refuse; the message below still reaches the page.
        }
        if (session) open.postMessage({ type: 'open-session', session });
        return;
      }
      await self.clients.openWindow(session ? `/#s=${session}` : '/');
    })(),
  );
});
