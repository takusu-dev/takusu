// DeleteConfirmButton — two-tap delete button to prevent accidental deletes.
// First tap arms (red background, trash icon), second tap fires onConfirm.
// Auto-disarms after 3s.

import { useEffect, useRef, useState } from 'react';
import { Pressable, StyleSheet } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { COLORS } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

interface DeleteConfirmButtonProps {
  onConfirm: () => void;
  size?: number;
  iconSize?: number;
  hitSlop?: number;
}

export function DeleteConfirmButton({
  onConfirm,
  size = 40,
  iconSize = 22,
  hitSlop,
}: DeleteConfirmButtonProps) {
  const [armed, setArmed] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return (
    <Pressable
      style={[
        styles.button,
        {
          width: size,
          height: size,
          borderRadius: size / 2,
        },
        armed && { backgroundColor: COLORS.red },
      ]}
      hitSlop={hitSlop}
      onPress={() => {
        if (armed) {
          if (timerRef.current) clearTimeout(timerRef.current);
          onConfirm();
          setArmed(false);
        } else {
          haptic.medium();
          setArmed(true);
          timerRef.current = setTimeout(() => setArmed(false), 3000);
        }
      }}
    >
      <Ionicons
        name={armed ? 'trash' : 'trash-outline'}
        size={iconSize}
        color={armed ? COLORS.white : COLORS.red}
      />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  button: {
    alignItems: 'center',
    justifyContent: 'center',
  },
});
