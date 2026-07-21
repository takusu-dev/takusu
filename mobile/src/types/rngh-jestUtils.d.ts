declare module 'react-native-gesture-handler/lib/commonjs/jestUtils' {
  export function getByGestureTestId(testID: string): any;
  export function fireGestureHandler(
    componentOrGesture: any,
    eventList?: any[],
  ): void;
}
