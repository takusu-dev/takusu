// Dynamic Expo config.
//
// Stable builds use the values in app.json unchanged. To build a development
// variant that can coexist with the stable app on the same device (different
// application ID + launcher label + deep-link scheme), run the build with:
//
//   TAKUSU_BUILD_VARIANT=dev npx expo prebuild --platform android --no-install
//
// or use the Nix helper:
//
//   nix run .#build-android-apk-dev
//
// This keeps the stable package (`dev.satler.takusu`) intact for release CI,
// which never sets TAKUSU_BUILD_VARIANT.
const { withSentry } = require('@sentry/react-native/expo');
const withTakusuAppIcon = require('./plugins/withTakusuAppIcon');

const baseConfig = require('./app.json');
const expo = baseConfig.expo;

const isDev = process.env.TAKUSU_BUILD_VARIANT === 'dev';

// Embed git commit/tag at build time so the settings page can show the
// exact source the APK was built from (instead of an opaque build number).
// Falls back to "unknown" when the env vars are not set (e.g. local dev).
expo.extra = {
  ...(expo.extra || {}),
  gitCommit: process.env.TAKUSU_GIT_COMMIT || 'unknown',
  gitTag: process.env.TAKUSU_GIT_TAG || 'unknown',
};

if (isDev) {
  expo.name = 'takusu dev';
  expo.slug = 'takusu-dev';
  expo.scheme = 'takusu-dev';
  if (expo.android) {
    expo.android.package = 'dev.satler.takusu.dev';
  }
}

module.exports = withTakusuAppIcon(
  withSentry(baseConfig, {
    url: process.env.SENTRY_URL || 'https://sentry.io/',
    project: process.env.SENTRY_PROJECT || 'takusu',
    organization: process.env.SENTRY_ORG || 'satler-git',
  }),
);
