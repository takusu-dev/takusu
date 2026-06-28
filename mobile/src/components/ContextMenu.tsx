// ContextMenu — hamburger menu button + dropdown
// Always available: Settings, Undo/Redo
// When tasks are selected: Reschedule selected, Reschedule others, Delete, Create dependent task, Clear selection

import { useState } from 'react';
import { Modal, Pressable, StyleSheet, Text, View } from 'react-native';
import { COLORS } from '@/src/theme';
import { undoRedo } from '@/src/api/undoRedo';

interface ContextMenuProps {
  hasSelection: boolean;
  onSettings: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onRescheduleSelected: () => void;
  onRescheduleOthers: () => void;
  onDeleteSelected: () => void;
  onCreateDependent: () => void;
  onClearSelection: () => void;
}

export function ContextMenu({
  hasSelection,
  onSettings,
  onUndo,
  onRedo,
  onRescheduleSelected,
  onRescheduleOthers,
  onDeleteSelected,
  onCreateDependent,
  onClearSelection,
}: ContextMenuProps) {
  const [open, setOpen] = useState(false);

  function item(label: string, onPress: () => void, disabled?: boolean) {
    return (
      <Pressable
        key={label}
        style={({ pressed }) => [
          styles.menuItem,
          pressed && styles.menuItemPressed,
          disabled && styles.menuItemDisabled,
        ]}
        disabled={disabled}
        onPress={() => {
          setOpen(false);
          onPress();
        }}
      >
        <Text style={[styles.menuItemText, disabled && styles.menuItemTextDisabled]}>
          {label}
        </Text>
      </Pressable>
    );
  }

  return (
    <>
      <Pressable
        style={({ pressed }) => [styles.button, pressed && styles.buttonPressed]}
        onPress={() => setOpen(true)}
      >
        <Text style={styles.buttonText}>☰</Text>
      </Pressable>

      <Modal visible={open} transparent animationType="fade">
        <Pressable style={styles.overlay} onPress={() => setOpen(false)}>
          <View style={styles.menu}>
            {item('設定', onSettings)}
            {item(
              `元に戻す${undoRedo.canUndo() ? '' : ' (なし)'}`,
              onUndo,
              !undoRedo.canUndo(),
            )}
            {item(
              `やり直し${undoRedo.canRedo() ? '' : ' (なし)'}`,
              onRedo,
              !undoRedo.canRedo(),
            )}

            {hasSelection && <View style={styles.separator} />}
            {hasSelection &&
              item('選択以外をreschedule', onRescheduleOthers)}
            {hasSelection &&
              item('選択をreschedule', onRescheduleSelected)}
            {hasSelection && item('削除', onDeleteSelected)}
            {hasSelection && item('依存とする新規タスク作成', onCreateDependent)}
            {hasSelection && item('選択解除', onClearSelection)}
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
  buttonText: {
    fontSize: 22,
    color: COLORS.brand,
  },
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.2)',
  },
  menu: {
    position: 'absolute',
    top: 60,
    left: 12,
    backgroundColor: COLORS.white,
    borderRadius: 12,
    paddingVertical: 4,
    minWidth: 220,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.3,
    shadowRadius: 8,
    elevation: 8,
  },
  menuItem: {
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
    color: COLORS.black,
  },
  menuItemTextDisabled: {
    color: COLORS.gray,
  },
  separator: {
    height: 1,
    backgroundColor: COLORS.separator,
    marginVertical: 4,
    marginHorizontal: 12,
  },
});
