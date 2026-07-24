import { NativeModule, requireNativeModule } from 'expo';

export type AppIconTheme = 'light' | 'dark' | 'catppuccin' | 'aura-soft-dark';

interface TakusuAppIconModuleType extends NativeModule {
  setTheme(theme: AppIconTheme): boolean;
  getAvailableThemes(): AppIconTheme[];
}

const nativeModule =
  requireNativeModule<TakusuAppIconModuleType>('TakusuAppIcon');

export default nativeModule;
