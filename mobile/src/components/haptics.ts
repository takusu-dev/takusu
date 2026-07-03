// Thin wrappers around expo-haptics so call-sites stay one-liners and the
// haptic "vocabulary" stays consistent across the app.
//
// Vocabulary:
//   haptic.light()    — subtle taps, navigation, opening menus/modals
//   haptic.medium()   — confirmations, mode changes, destructive actions
//   haptic.select()   — toggles, tab switches, picker/chip selections
//   haptic.success()  — successful operations (copy, undo/redo, task done)
//   haptic.warning()  — cancellations / non-fatal warnings
//
// Errors are swallowed: haptics are best-effort and must never break a flow.

import * as Haptics from 'expo-haptics';

function safe<T>(p: Promise<T>): void {
  p.catch(() => {});
}

export const haptic = {
  light(): void {
    safe(Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light));
  },
  medium(): void {
    safe(Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium));
  },
  heavy(): void {
    safe(Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Heavy));
  },
  select(): void {
    safe(Haptics.selectionAsync());
  },
  success(): void {
    safe(Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success));
  },
  warning(): void {
    safe(Haptics.notificationAsync(Haptics.NotificationFeedbackType.Warning));
  },
  error(): void {
    safe(Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error));
  },
};
