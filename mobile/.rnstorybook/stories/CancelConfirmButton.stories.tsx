import type { Meta, StoryObj } from '@storybook/react-native';

import { CancelConfirmButton } from '@/src/components/CancelConfirmButton';

const meta = {
  component: CancelConfirmButton,
} satisfies Meta<typeof CancelConfirmButton>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    onConfirm: () => {
      // eslint-disable-next-line no-console
      console.log('confirmed');
    },
  },
};
