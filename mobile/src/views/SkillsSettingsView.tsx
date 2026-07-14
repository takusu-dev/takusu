import { useCallback, useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useServer } from '@/src/api/ServerProvider';
import type { SkillRow, CreateSkill, UpdateSkill } from '@/src/api/types';
import { useColors, BRAND_COLOR } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

export function SkillsSettingsView() {
  const { client } = useServer();
  const colors = useColors();

  const [skills, setSkills] = useState<SkillRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<SkillRow | null>(null);
  const [slug, setSlug] = useState('');
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [body, setBody] = useState('');

  const load = useCallback(async () => {
    if (!client) return;
    setLoading(true);
    try {
      const list = await client.listSkills();
      setSkills(list);
    } catch (e) {
      Alert.alert(
        'エラー',
        `読み込みに失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setLoading(false);
    }
  }, [client]);

  useEffect(() => {
    load();
  }, [load]);

  function openCreate() {
    haptic.light();
    setEditing(null);
    setSlug('');
    setName('');
    setDescription('');
    setBody('');
    setFormOpen(true);
  }

  function openEdit(skill: SkillRow) {
    if (skill.built_in) {
      return;
    }
    haptic.light();
    setEditing(skill);
    setSlug(skill.slug);
    setName(skill.name);
    setDescription(skill.description);
    setBody(skill.body);
    setFormOpen(true);
  }

  function closeForm() {
    setFormOpen(false);
    setEditing(null);
  }

  function normalizeSlug(text: string) {
    return text
      .toLowerCase()
      .replace(/[^a-z0-9-_]/g, '')
      .slice(0, 64);
  }

  function validate(): string | null {
    const s = slug.trim();
    if (!editing) {
      if (!s) return 'slug は必須です';
      if (!/^[a-z0-9_-]+$/.test(s)) {
        return 'slug は小文字英数字、-、_ のみ使用できます';
      }
      if (!name.trim()) return 'name は必須です';
      if (!body.trim()) return 'body は必須です';
    } else if (!name.trim() && !description.trim() && !body.trim()) {
      return 'name, description, body のいずれかを入力してください';
    }
    if (name.trim().length > 100) return 'name は100文字以下にしてください';
    if (description.trim().length > 500) {
      return 'description は500文字以下にしてください';
    }
    if (body.trim().length > 64 * 1024) {
      return 'body は64KB以下にしてください';
    }
    return null;
  }

  async function save() {
    if (!client) return;
    const error = validate();
    if (error) {
      Alert.alert('入力エラー', error);
      return;
    }
    try {
      if (editing) {
        const bodyUpdate: UpdateSkill = {
          name: name.trim() || undefined,
          description: description.trim() || undefined,
          body: body.trim() || undefined,
        };
        await client.updateSkill(editing.slug, bodyUpdate);
      } else {
        const create: CreateSkill = {
          slug: slug.trim(),
          name: name.trim(),
          description: description.trim(),
          body: body.trim(),
        };
        await client.createSkill(create);
      }
      haptic.success();
      closeForm();
      await load();
    } catch (e) {
      Alert.alert(
        'エラー',
        `保存に失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }

  async function remove(skill: SkillRow) {
    if (!client) return;
    if (skill.built_in) {
      Alert.alert('エラー', '組み込みスキルは削除できません');
      return;
    }
    Alert.alert('削除', `${skill.name} を削除しますか？`, [
      { text: 'キャンセル', style: 'cancel' },
      {
        text: '削除',
        style: 'destructive',
        onPress: async () => {
          try {
            await client.deleteSkill(skill.slug);
            haptic.success();
            await load();
          } catch (e) {
            Alert.alert(
              'エラー',
              `削除に失敗: ${e instanceof Error ? e.message : String(e)}`,
            );
          }
        },
      },
    ]);
  }

  return (
    <View style={styles.container}>
      <Pressable onPress={openCreate} style={styles.createButton}>
        <Text style={[styles.createButtonText, { color: BRAND_COLOR }]}>
          + 新しいスキル
        </Text>
      </Pressable>
      {loading ? (
        <ActivityIndicator color={BRAND_COLOR} />
      ) : (
        <ScrollView contentContainerStyle={styles.list}>
          {skills.map((skill) => (
            <View
              key={skill.slug}
              style={[styles.card, { borderBottomColor: colors.separator }]}
            >
              <Pressable
                onPress={() => openEdit(skill)}
                disabled={skill.built_in}
                style={styles.row}
              >
                <View style={styles.text}>
                  <Text style={[styles.name, { color: colors.black }]}>
                    {skill.name}
                  </Text>
                  <Text style={[styles.slug, { color: colors.gray }]}>
                    {skill.slug}
                    {skill.built_in ? ' (built-in)' : ''}
                  </Text>
                  <Text style={[styles.desc, { color: colors.black }]}>
                    {skill.description}
                  </Text>
                </View>
              </Pressable>
              {!skill.built_in && (
                <Pressable onPress={() => remove(skill)} style={styles.delete}>
                  <Text style={styles.deleteText}>削除</Text>
                </Pressable>
              )}
            </View>
          ))}
        </ScrollView>
      )}
      {formOpen && (
        <View style={styles.overlay}>
          <View style={[styles.modal, { backgroundColor: colors.white }]}>
            <Text style={[styles.title, { color: colors.black }]}>
              {editing ? 'スキルを編集' : 'スキルを作成'}
            </Text>
            <TextInput
              style={[
                styles.input,
                { color: colors.black, borderColor: colors.separator },
              ]}
              placeholder="slug (a-z, 0-9, -, _)"
              placeholderTextColor={colors.gray}
              value={slug}
              onChangeText={(text) => setSlug(normalizeSlug(text))}
              editable={!editing}
              maxLength={64}
              autoCapitalize="none"
            />
            <TextInput
              style={[
                styles.input,
                { color: colors.black, borderColor: colors.separator },
              ]}
              placeholder="name"
              placeholderTextColor={colors.gray}
              value={name}
              onChangeText={setName}
              maxLength={100}
            />
            <TextInput
              style={[
                styles.input,
                { color: colors.black, borderColor: colors.separator },
              ]}
              placeholder="description"
              placeholderTextColor={colors.gray}
              value={description}
              onChangeText={setDescription}
              maxLength={500}
            />
            <TextInput
              style={[
                styles.bodyInput,
                { color: colors.black, borderColor: colors.separator },
              ]}
              placeholder="body (markdown)"
              placeholderTextColor={colors.gray}
              value={body}
              onChangeText={setBody}
              multiline
              textAlignVertical="top"
              maxLength={64 * 1024}
            />
            <View style={styles.actions}>
              <Pressable onPress={closeForm} style={styles.cancel}>
                <Text style={styles.cancelText}>キャンセル</Text>
              </Pressable>
              <Pressable onPress={save} style={styles.save}>
                <Text style={styles.saveText}>保存</Text>
              </Pressable>
            </View>
          </View>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  createButton: { padding: 16 },
  createButtonText: { fontSize: 16, fontWeight: '700' },
  list: { padding: 16, gap: 12 },
  card: {
    flexDirection: 'row',
    alignItems: 'center',
    borderBottomWidth: 1,
    paddingVertical: 12,
  },
  row: { flex: 1, flexDirection: 'row' },
  text: { flex: 1, gap: 2 },
  name: { fontSize: 16, fontWeight: '700' },
  slug: { fontSize: 12 },
  desc: { fontSize: 13 },
  delete: { padding: 8 },
  deleteText: { color: '#B33A3A', fontWeight: '700' },
  overlay: {
    ...StyleSheet.absoluteFill,
    backgroundColor: 'rgba(0,0,0,0.4)',
    justifyContent: 'center',
    padding: 20,
  },
  modal: { borderRadius: 12, padding: 16, gap: 12 },
  title: { fontSize: 18, fontWeight: '700' },
  input: { borderWidth: 1, borderRadius: 8, padding: 10, minHeight: 44 },
  bodyInput: { borderWidth: 1, borderRadius: 8, padding: 10, minHeight: 120 },
  actions: { flexDirection: 'row', gap: 8, justifyContent: 'flex-end' },
  cancel: { padding: 10 },
  cancelText: { color: '#666' },
  save: { backgroundColor: BRAND_COLOR, borderRadius: 8, padding: 10 },
  saveText: { color: '#fff', fontWeight: '700' },
});
