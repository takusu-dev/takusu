// Redirect share intents to the iCal import handler route.
// expo-sharing uses the hostname "expo-sharing" for incoming share URLs.

export function redirectSystemPath({
  path,
}: {
  path: string;
  initial: boolean;
}): string {
  try {
    if (new URL(path).hostname === 'expo-sharing') {
      return '/import-ical';
    }
    return path;
  } catch {
    return path;
  }
}
