import { Text } from 'react-native';

export function Ionicons({
  name,
  size,
  color,
}: {
  name?: string;
  size?: number;
  color?: string;
}) {
  return <Text style={{ color, fontSize: size }}>{name}</Text>;
}
