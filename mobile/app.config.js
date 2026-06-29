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
const baseConfig = require("./app.json");
const expo = baseConfig.expo;

const isDev = process.env.TAKUSU_BUILD_VARIANT === "dev";

if (isDev) {
  expo.name = "takusu dev";
  expo.slug = "takusu-dev";
  expo.scheme = "takusu-dev";
  if (expo.ios) {
    expo.ios.bundleIdentifier = "dev.satler.takusu.dev";
  }
  if (expo.android) {
    expo.android.package = "dev.satler.takusu.dev";
  }
}

module.exports = baseConfig;
