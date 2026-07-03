// Simple event emitter for OAuth callback deep links.
// _layout.tsx listens for `takusu://oauth/callback?code=...` and emits here.
// SettingsView subscribes to complete the OAuth flow.

type OAuthCallbackListener = (code: string) => void;

let listener: OAuthCallbackListener | null = null;

export function setOAuthCallbackListener(
  fn: OAuthCallbackListener | null,
): void {
  listener = fn;
}

export function emitOAuthCallback(code: string): void {
  if (listener) {
    listener(code);
  }
}
