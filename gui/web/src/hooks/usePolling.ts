import { useEffect, useRef } from 'react';

/**
 * Calls `callback` on an interval, pausing when the browser tab is hidden.
 * Also calls `callback` immediately when the tab becomes visible again.
 */
export function usePolling(callback: () => void, intervalMs: number) {
  const savedCallback = useRef(callback);
  savedCallback.current = callback;

  useEffect(() => {
    let id: ReturnType<typeof setInterval> | null = null;

    function start() {
      if (id === null) {
        savedCallback.current();
        id = setInterval(() => savedCallback.current(), intervalMs);
      }
    }

    function stop() {
      if (id !== null) {
        clearInterval(id);
        id = null;
      }
    }

    function onVisibilityChange() {
      if (document.hidden) {
        stop();
      } else {
        start();
      }
    }

    document.addEventListener('visibilitychange', onVisibilityChange);

    if (!document.hidden) {
      start();
    }

    return () => {
      stop();
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, [intervalMs]);
}
