// ContextMenu — hamburger menu button + dropdown
// Always available: Settings, Undo/Redo
// When items are selected: Delete, Clear selection (and Select all when
//   onSelectAll is provided). Task-specific actions (Reschedule selected,
//   Reschedule others, Create dependent task) are only shown when their
//   handlers are provided.

import { useState } from 'react';
import { Modal, Pressable, StyleSheet, Text, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { undoRedo } from '@/src/api/undoRedo';
import { haptic } from '@/src/components/haptics';

interface ContextMenuProps {
  hasSelection: boolean;
  onSettings: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onClearSelection: () => void;
  onDeleteSelected: () => void;
  // Optional handlers — only rendered when provided
  onSelectAll?: () => void;
  onRescheduleSelected?: () => void;
  onRescheduleOthers?: () => void;
  onCreateDependent?: () => void;
}

type MenuItem = {
  label: string;
  icon: keyof typeof Ionicons.glyphMap;
  onPress: () => void;
  disabled?: boolean;
  danger?: boolean;
};

export function ContextMenu({
  hasSelection,
  onSettings,
  onUndo,
  onRedo,
  onClearSelection,
  onDeleteSelected,
  onSelectAll,
  onRescheduleSelected,
  onRescheduleOthers,
  onCreateDependent,
}: ContextMenuProps) {
  const colors = useColors();
  const [open, setOpen] = useState(false);

  const alwaysItems: MenuItem[] = [
    { label: '設定', icon: 'settings-outline', onPress: onSettings },
    {
      label: `元に戻す${undoRedo.canUndo() ? '' : ' (なし)'}`,
      icon: 'arrow-undo-outline',
      onPress: onUndo,
      disabled: !undoRedo.canUndo(),
    },
    {
      label: `やり直し${undoRedo.canRedo() ? '' : ' (なし)'}`,
      icon: 'arrow-redo-outline',
      onPress: onRedo,
      disabled: !undoRedo.canRedo(),
    },
  ];

  const selectionItems: MenuItem[] = hasSelection
    ? [
        ...(onSelectAll
          ? [
              {
                label: 'すべて選択',
                icon: 'checkbox-outline' as const,
                onPress: onSelectAll,
              },
            ]
          : []),
        {
          label: '選択解除',
          icon: 'close-circle-outline',
          onPress: onClearSelection,
        },
        ...(onRescheduleOthers
          ? [
              {
                label: '選択以外をreschedule',
                icon: 'calendar-outline' as const,
                onPress: onRescheduleOthers,
              },
            ]
          : []),
        ...(onRescheduleSelected
          ? [
              {
                label: '選択をreschedule',
                icon: 'calendar-number-outline' as const,
                onPress: onRescheduleSelected,
              },
            ]
          : []),
        ...(onCreateDependent
          ? [
              {
                label: '依存とする新規タスク作成',
                icon: 'git-branch-outline' as const,
                onPress: onCreateDependent,
              },
            ]
          : []),
        {
          label: '削除',
          icon: 'trash-outline',
          onPress: onDeleteSelected,
          danger: true,
        },
      ]
    : [];

  function renderItem(item: MenuItem) {
    return (
      <Pressable
        key={item.label}
        style={({ pressed }) => [
          styles.menuItem,
          pressed && styles.menuItemPressed,
          item.disabled && styles.menuItemDisabled,
        ]}
        disabled={item.disabled}
        onPress={() => {
          if (item.danger) haptic.medium();
          else haptic.light();
          setOpen(false);
          item.onPress();
        }}
      >
        <Ionicons
          name={item.icon}
          size={20}
          color={
            item.danger ? COLORS.red : item.disabled ? colors.gray : BRAND_COLOR
          }
        />
        <Text
          style={[
            styles.menuItemText,
            { color: item.danger ? COLORS.red : colors.black },
            item.disabled && styles.menuItemTextDisabled,
          ]}
        >
          {item.label}
        </Text>
      </Pressable>
    );
  }

  return (
    <>
      <Pressable
        style={({ pressed }) => [
          styles.button,
          pressed && styles.buttonPressed,
        ]}
        onPress={() => {
          haptic.light();
          setOpen(true);
        }}
      >
        <Ionicons name="menu" size={24} color={BRAND_COLOR} />
      </Pressable>

      <Modal visible={open} transparent animationType="fade">
        <Pressable style={styles.overlay} onPress={() => setOpen(false)}>
          <View style={[styles.menu, { backgroundColor: colors.white }]}>
            {alwaysItems.map(renderItem)}
            {hasSelection && (
              <View
                style={[
                  styles.separator,
                  { backgroundColor: colors.separator },
                ]}
              />
            )}
            {selectionItems.map(renderItem)}
          </View>
        </Pressable>
      </Modal>
    </>
  );
}

const styles = StyleSheet.create({
  button: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  buttonPressed: {
    backgroundColor: 'rgba(114,97,163,0.1)',
  },
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.2)',
  },
  menu: {
    position: 'absolute',
    top: 60,
    left: 12,
    borderRadius: 12,
    paddingVertical: 4,
    minWidth: 240,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.3,
    shadowRadius: 8,
    elevation: 8,
  },
  menuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingVertical: 12,
    paddingHorizontal: 16,
  },
  menuItemPressed: {
    backgroundColor: 'rgba(114,97,163,0.1)',
  },
  menuItemDisabled: {
    opacity: 0.4,
  },
  menuItemText: {
    fontSize: 15,
  },
  menuItemTextDisabled: {
    opacity: 0.5,
  },
  separator: {
    height: 1,
    marginVertical: 4,
    marginHorizontal: 12,
  },
});
