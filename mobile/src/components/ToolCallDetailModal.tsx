import { useCallback, useMemo, type ReactNode } from 'react';
import {
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { BRAND_COLOR, COLORS, useColors, type ColorSet } from '@/src/theme';
import { haptic } from '@/src/components/haptics';
import { formatJson } from '@/src/utils/formatJson';
import type { ToolCallItem } from '@/src/api/agentSessionStore';

const MAX_DEPTH = 3;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function renderValue(value: unknown, colors: ColorSet, depth = 0): ReactNode {
  if (value === undefined || value === null) {
    return <Text style={[styles.valueText, { color: colors.gray }]}>-</Text>;
  }
  if (
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
  ) {
    return (
      <Text style={[styles.valueText, { color: colors.black }]}>
        {String(value)}
      </Text>
    );
  }
  if (depth < MAX_DEPTH) {
    if (isRecord(value)) {
      return <ObjectRows object={value} colors={colors} depth={depth} />;
    }
    if (Array.isArray(value)) {
      return <ArrayRows array={value} colors={colors} depth={depth} />;
    }
  }
  return (
    <Text style={[styles.monoText, { color: colors.black }]}>
      {formatJson(value)}
    </Text>
  );
}

interface ValueRowProps {
  label: string;
  value: ReactNode;
  colors: ColorSet;
}

function ValueRow({ label, value, colors }: ValueRowProps) {
  return (
    <View style={styles.row}>
      <Text style={[styles.rowLabel, { color: colors.gray }]}>{label}</Text>
      <View style={styles.rowValue}>{value}</View>
    </View>
  );
}

interface ObjectRowsProps {
  object: Record<string, unknown>;
  colors: ColorSet;
  depth?: number;
}

function ObjectRows({ object, colors, depth = 0 }: ObjectRowsProps) {
  const isNested = depth > 0;
  return (
    <View
      style={[
        styles.sectionBox,
        isNested && styles.nestedSectionBox,
        {
          backgroundColor: colors.surface,
          borderColor: colors.separator,
          marginLeft: isNested ? 12 : 0,
        },
      ]}
    >
      {Object.entries(object).map(([key, value]) => (
        <ValueRow
          key={key}
          label={key}
          value={renderValue(value, colors, depth + 1)}
          colors={colors}
        />
      ))}
    </View>
  );
}

interface ArrayRowsProps {
  array: unknown[];
  colors: ColorSet;
  depth?: number;
}

function ArrayRows({ array, colors, depth = 0 }: ArrayRowsProps) {
  const isNested = depth > 0;
  return (
    <View
      style={[
        styles.sectionBox,
        isNested && styles.nestedSectionBox,
        {
          backgroundColor: colors.surface,
          borderColor: colors.separator,
          marginLeft: isNested ? 12 : 0,
        },
      ]}
    >
      {array.map((item, index) => (
        <ValueRow
          key={index}
          label={String(index)}
          value={renderValue(item, colors, depth + 1)}
          colors={colors}
        />
      ))}
    </View>
  );
}

interface AsrArgumentListProps {
  questions: unknown[];
  colors: ColorSet;
}

function AsrArgumentList({ questions, colors }: AsrArgumentListProps) {
  const records = questions.filter(isRecord);
  return (
    <View
      style={[
        styles.sectionBox,
        {
          backgroundColor: colors.surface,
          borderColor: colors.separator,
        },
      ]}
    >
      {records.map((item, index) => (
        <View key={index} style={styles.asrItem}>
          <Text style={[styles.asrOriginal, { color: colors.black }]}>
            {String(item.text ?? '')}
          </Text>
          <Text style={[styles.asrPurpose, { color: colors.gray }]}>
            目的: {String(item.for ?? '')}
          </Text>
        </View>
      ))}
    </View>
  );
}

interface ToolCallDetailModalProps {
  visible: boolean;
  call: ToolCallItem | null;
  onClose: () => void;
}

export function ToolCallDetailModal({
  visible,
  call,
  onClose,
}: ToolCallDetailModalProps) {
  const colors = useColors();

  const { title, status, statusColor, isAsr, args } = useMemo<{
    title: string;
    status: string;
    statusColor: string;
    isAsr: boolean;
    args: unknown;
  }>(() => {
    if (!call) {
      return {
        title: '',
        status: '',
        statusColor: colors.gray,
        isAsr: false,
        args: undefined,
      };
    }
    const asr = call.name === 'correct_asr';
    const done = call.result !== undefined;
    return {
      title: asr ? 'ASR訂正' : call.name,
      status: done ? (call.isError ? 'エラー' : '成功') : '実行中',
      statusColor: done
        ? call.isError
          ? colors.red
          : colors.green
        : colors.gray,
      isAsr: asr,
      args: call.arguments,
    };
  }, [call, colors]);

  const handleCopy = useCallback(async () => {
    if (!call) return;
    const text = formatJson({
      name: call.name,
      arguments: call.arguments,
      result: call.result,
      isError: call.isError,
    });
    if (!text) return;
    try {
      await Clipboard.setStringAsync(text);
      haptic.success();
    } catch {
      haptic.error();
    }
  }, [call]);

  if (!call) return null;

  return (
    <Modal
      visible={visible}
      transparent
      animationType="fade"
      onRequestClose={onClose}
    >
      <View style={styles.overlay}>
        <Pressable style={styles.backdrop} onPress={onClose} />
        <View style={[styles.card, { backgroundColor: colors.white }]}>
          <View style={styles.header}>
            <View
              style={[styles.statusDot, { backgroundColor: statusColor }]}
            />
            <Text style={[styles.title, { color: colors.black }]}>{title}</Text>
            <View style={[styles.badge, { backgroundColor: statusColor }]}>
              <Text style={[styles.badgeText, { color: colors.white }]}>
                {status}
              </Text>
            </View>
          </View>

          <ScrollView
            style={styles.body}
            contentContainerStyle={styles.bodyContent}
          >
            {args !== undefined && (
              <View style={styles.section}>
                <Text style={[styles.sectionTitle, { color: colors.gray }]}>
                  引数
                </Text>
                {isAsr && isRecord(args) && Array.isArray(args.questions) ? (
                  <AsrArgumentList questions={args.questions} colors={colors} />
                ) : isRecord(args) ? (
                  <ObjectRows object={args} colors={colors} depth={0} />
                ) : (
                  <Text style={[styles.monoText, { color: colors.black }]}>
                    {formatJson(args)}
                  </Text>
                )}
              </View>
            )}

            {call.result !== undefined && (
              <View style={styles.section}>
                <Text
                  style={[
                    styles.sectionTitle,
                    { color: call.isError ? colors.red : colors.gray },
                  ]}
                >
                  {call.isError ? 'エラー' : '結果'}
                </Text>
                <ResultContent result={call.result} colors={colors} />
              </View>
            )}
          </ScrollView>

          <View style={styles.footer}>
            <Pressable
              onPress={handleCopy}
              style={[
                styles.copyButton,
                {
                  borderColor: colors.separator,
                  backgroundColor: colors.surface,
                },
              ]}
            >
              <Text style={[styles.copyText, { color: colors.black }]}>
                JSONをコピー
              </Text>
            </Pressable>
            <Pressable
              onPress={onClose}
              style={[styles.closeButton, { backgroundColor: BRAND_COLOR }]}
            >
              <Text style={styles.closeText}>閉じる</Text>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}

function ResultContent({
  result,
  colors,
}: {
  result: string;
  colors: ColorSet;
}) {
  let parsed: unknown = result;
  try {
    parsed = JSON.parse(result);
  } catch {
    parsed = result;
  }
  if (isRecord(parsed)) {
    return <ObjectRows object={parsed} colors={colors} depth={0} />;
  }
  if (Array.isArray(parsed)) {
    return <ArrayRows array={parsed} colors={colors} depth={0} />;
  }
  return (
    <View
      style={[
        styles.sectionBox,
        {
          backgroundColor: colors.surface,
          borderColor: colors.separator,
        },
      ]}
    >
      {renderValue(parsed, colors, 0)}
    </View>
  );
}

const styles = StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.4)',
    justifyContent: 'center',
    alignItems: 'center',
    padding: 20,
  },
  backdrop: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    bottom: 0,
  },
  card: {
    width: '100%',
    height: '80%',
    borderRadius: 16,
    padding: 16,
    gap: 12,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
  },
  statusDot: { width: 10, height: 10, borderRadius: 5 },
  title: { flex: 1, fontSize: 18, fontWeight: '700' },
  badge: { paddingHorizontal: 8, paddingVertical: 3, borderRadius: 12 },
  badgeText: { fontSize: 11, fontWeight: '700' },
  body: { flex: 1 },
  bodyContent: { gap: 14 },
  section: { gap: 6 },
  sectionTitle: { fontSize: 12, fontWeight: '600' },
  sectionBox: {
    borderWidth: 1,
    borderRadius: 10,
    padding: 10,
    gap: 8,
  },
  nestedSectionBox: {
    borderRadius: 8,
    padding: 8,
    gap: 6,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 8,
  },
  rowLabel: {
    minWidth: 80,
    fontSize: 13,
    fontWeight: '600',
  },
  rowValue: { flex: 1 },
  valueText: { fontSize: 13 },
  monoText: { fontSize: 11, fontFamily: 'monospace' },
  asrItem: { gap: 2 },
  asrOriginal: { fontSize: 13, fontWeight: '600' },
  asrPurpose: { fontSize: 12 },
  footer: { gap: 10 },
  copyButton: {
    padding: 10,
    borderRadius: 8,
    alignItems: 'center',
    borderWidth: 1,
  },
  copyText: { fontSize: 13, fontWeight: '600' },
  closeButton: {
    padding: 12,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  closeText: { color: COLORS.white, fontWeight: '700' },
});
