import { useRef, useCallback } from "react";

/**
 * Tracks IME composition state reliably across browsers.
 *
 * In Chrome/WebKit, `compositionend` fires *before* the `keydown` for the
 * Enter key that confirms the IME input, so `e.nativeEvent.isComposing` is
 * already `false` by the time the handler runs. This hook keeps a ref-based
 * flag that stays `true` until the next event-loop tick after `compositionend`,
 * giving keydown handlers a reliable way to ignore the confirming Enter.
 */
export function useIMEComposition() {
  const composingRef = useRef(false);

  const onCompositionStart = useCallback(() => {
    composingRef.current = true;
  }, []);

  const onCompositionEnd = useCallback(() => {
    // Delay clearing to the next tick so the keydown handler that fires
    // in the same event loop iteration still sees composing = true.
    requestAnimationFrame(() => {
      composingRef.current = false;
    });
  }, []);

  return {
    isComposingRef: composingRef,
    compositionProps: {
      onCompositionStart,
      onCompositionEnd,
    },
  };
}
