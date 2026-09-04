import { useCallback, useEffect, useRef, useState } from "react";
import { moveItemInList } from "./launcherToolOrder";

export function useCardDragReorder(params: {
  ids: string[];
  onReorder: (ids: string[]) => void;
  longPressMs?: number;
}) {
  const { ids, onReorder, longPressMs = 300 } = params;

  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);

  const dragActiveRef = useRef(false);
  const draggingIdRef = useRef<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearPressTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const endDrag = useCallback(() => {
    dragActiveRef.current = false;
    draggingIdRef.current = null;
    setDraggingId(null);
    setDragOverId(null);
  }, []);

  useEffect(() => {
    if (!draggingId) return;
    const finalize = () => {
      endDrag();
    };
    window.addEventListener("pointerup", finalize);
    window.addEventListener("pointercancel", finalize);
    return () => {
      window.removeEventListener("pointerup", finalize);
      window.removeEventListener("pointercancel", finalize);
    };
  }, [draggingId, endDrag]);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent, id: string) => {
      e.preventDefault();
      clearPressTimer();
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        dragActiveRef.current = true;
        draggingIdRef.current = id;
        setDraggingId(id);
      }, longPressMs);
    },
    [clearPressTimer, longPressMs],
  );

  const handlePointerUp = useCallback(() => {
    clearPressTimer();
  }, [clearPressTimer]);

  const handleCardPointerOver = useCallback(
    (targetId: string) => {
      if (!dragActiveRef.current) return;
      const dragged = draggingIdRef.current;
      if (!dragged || dragged === targetId) return;
      setDragOverId(targetId);
      const from = ids.indexOf(dragged);
      const to = ids.indexOf(targetId);
      if (from < 0 || to < 0 || from === to) return;
      onReorder(moveItemInList(ids, from, to));
    },
    [ids, onReorder],
  );

  return {
    draggingId,
    dragOverId,
    handlePointerDown,
    handlePointerUp,
    handleCardPointerOver,
  };
}