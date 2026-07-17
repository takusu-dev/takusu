import type { StorybookConfig } from '@storybook/react-native';

const main: StorybookConfig = {
  stories: [
    '../src/components/**/*.stories.?(ts|tsx|js|jsx)',
    './stories/**/*.stories.?(ts|tsx|js|jsx)',
  ],
  deviceAddons: ['@storybook/addon-ondevice-actions'],
};

export default main;
