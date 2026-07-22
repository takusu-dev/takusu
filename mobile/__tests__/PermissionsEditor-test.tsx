import { useState } from 'react';
import { render, fireEvent, waitFor } from '@testing-library/react-native';

import { ThemeProvider } from '@/src/theme';
import type { PermissionsMap } from '@/src/api/settingsStore';
import { PermissionsEditor } from '@/src/components/PermissionsEditor';

function TestWrapper({
  initialPermissions,
  onChange,
}: {
  initialPermissions?: PermissionsMap;
  onChange?: (permissions: PermissionsMap) => void;
}) {
  const [permissions, setPermissions] = useState(initialPermissions ?? {});

  return (
    <ThemeProvider>
      <PermissionsEditor
        permissions={permissions}
        onChange={(next) => {
          setPermissions(next);
          onChange?.(next);
        }}
      />
    </ThemeProvider>
  );
}

async function setup(
  overrides: Partial<{
    initialPermissions: PermissionsMap;
    onChange: (permissions: PermissionsMap) => void;
  }> = {},
) {
  const onChange = jest.fn();
  const utils = await render(
    <TestWrapper
      initialPermissions={overrides.initialPermissions}
      onChange={(next) => {
        onChange(next);
        overrides.onChange?.(next);
      }}
    />,
  );
  return { ...utils, onChange };
}

describe('PermissionsEditor', () => {
  it('renders categories and permissions', async () => {
    const { getByText } = await setup();
    expect(getByText('タスク')).toBeTruthy();
    expect(getByText('タスク作成')).toBeTruthy();
    expect(getByText('習慣')).toBeTruthy();
    expect(getByText('記憶作成')).toBeTruthy();
  });

  it('toggles an individual permission via row press', async () => {
    const { getByText, onChange } = await setup();
    await fireEvent.press(getByText('タスク作成'));
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ 'task:create': true }),
    );
  });

  it('toggles a category switch to enable all permissions in the category', async () => {
    const { getByRole, onChange } = await setup();
    const taskCategorySwitch = getByRole('switch', { name: 'タスク' });
    await fireEvent(taskCategorySwitch, 'valueChange', true);
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        'task:create': true,
        'task:update': true,
        'task:delete': true,
        'task:move': true,
        'task:start': true,
        'task:pause': true,
        'task:progress': true,
        'task:complete': true,
        'task:split': true,
      }),
    );
  });

  it('toggles a category switch to disable all permissions in the category', async () => {
    const { getByRole, onChange } = await setup({
      initialPermissions: { 'task:create': true, 'task:update': true },
    });
    const taskCategorySwitch = getByRole('switch', { name: 'タスク' });
    await fireEvent(taskCategorySwitch, 'valueChange', true);
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        'task:create': true,
        'task:update': true,
        'task:delete': true,
        'task:move': true,
        'task:start': true,
        'task:pause': true,
        'task:progress': true,
        'task:complete': true,
        'task:split': true,
      }),
    );
    await fireEvent(taskCategorySwitch, 'valueChange', false);
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        'task:create': false,
        'task:update': false,
        'task:delete': false,
        'task:move': false,
        'task:start': false,
        'task:pause': false,
        'task:progress': false,
        'task:complete': false,
        'task:split': false,
      }),
    );
  });

  it('master ON enables all individual switches and disables them', async () => {
    const { getByRole, onChange } = await setup();
    const masterSwitch = getByRole('switch', { name: 'すべて自動承認' });
    await fireEvent(masterSwitch, 'valueChange', true);
    expect(onChange).toHaveBeenLastCalledWith({ '*:*': true });

    const taskCreateSwitch = getByRole('switch', { name: 'タスク作成' });
    expect(taskCreateSwitch).toBeChecked();
    expect(taskCreateSwitch).toBeDisabled();
  });

  it('ignores individual permission press while master is ON', async () => {
    const { getByText, getByRole, onChange } = await setup();
    const masterSwitch = getByRole('switch', { name: 'すべて自動承認' });
    await fireEvent(masterSwitch, 'valueChange', true);
    onChange.mockClear();

    await fireEvent.press(getByText('タスク作成'));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('filters permissions by search', async () => {
    const { getByPlaceholderText, queryByText } = await setup();
    await fireEvent.changeText(getByPlaceholderText('権限を検索'), '作成');
    await waitFor(() => {
      expect(queryByText('タスク作成')).toBeTruthy();
      expect(queryByText('タスク更新')).toBeNull();
      expect(queryByText('習慣作成')).toBeTruthy();
    });
  });

  it('reflects target wildcard permissions in switches', async () => {
    const { getByRole } = await setup({
      initialPermissions: { 'task:*': true },
    });
    expect(getByRole('switch', { name: 'タスク作成' })).toBeChecked();
    expect(getByRole('switch', { name: 'タスク更新' })).toBeChecked();
    expect(getByRole('switch', { name: '習慣作成' })).not.toBeChecked();
  });

  it('toggles all permissions in a category even when search is active', async () => {
    const { getByRole, getByPlaceholderText, onChange } = await setup();
    await fireEvent.changeText(
      getByPlaceholderText('権限を検索'),
      'タスク作成',
    );
    const taskCategorySwitch = getByRole('switch', { name: 'タスク' });
    await fireEvent(taskCategorySwitch, 'valueChange', true);
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        'task:create': true,
        'task:update': true,
        'task:delete': true,
        'task:move': true,
        'task:start': true,
        'task:pause': true,
        'task:progress': true,
        'task:complete': true,
        'task:split': true,
      }),
    );
  });
});
