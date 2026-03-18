import { useCallback, useEffect, useRef, useState } from 'react';
import type { DashboardBlock } from './types';
import { BLOCK_LABELS, snap } from './types';
import styles from './BlockContainer.module.css';

interface Props {
  block: DashboardBlock;
  children: React.ReactNode;
  onMove: (id: string, x: number, y: number) => void;
  onResize: (id: string, width: number, height: number) => void;
  onDelete: (id: string) => void;
}

type Interaction = 'move' | 'resize-right' | 'resize-bottom' | 'resize-corner';

export function BlockContainer({
  block,
  children,
  onMove,
  onResize,
  onDelete,
}: Props): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null);
  const [interaction, setInteraction] = useState<Interaction | null>(null)
  const startRef = useRef({ mouseX: 0, mouseY: 0, blockX: 0, blockY: 0, width: 0, height: 0 });
  const rafRef = useRef(0)

  const startDrag = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('button')) return;
    e.preventDefault();

    startRef.current = {
      mouseX: e.clientX, mouseY: e.clientY,
      blockX: block.x, blockY: block.y,
      width: 0, height: 0,
    };
    setInteraction('move');
  }, [block.x, block.y]);

  function startResize(e: React.MouseEvent, kind: 'resize-right' | 'resize-bottom' | 'resize-corner') {
    e.preventDefault();
    e.stopPropagation();
    if (!ref.current) return;

    const rect = ref.current.getBoundingClientRect();
    startRef.current = {
      mouseX: e.clientX, mouseY: e.clientY,
      blockX: 0, blockY: 0,
      width: rect.width, height: rect.height,
    }
    setInteraction(kind);
  }

  // the drag/resize effect
  useEffect(() => {
    if (!interaction) return;

    const onMouseMove = (evt: MouseEvent) => {
      cancelAnimationFrame(rafRef.current);

      rafRef.current = requestAnimationFrame(() => {
        const dx = evt.clientX - startRef.current.mouseX;
        const dy = evt.clientY - startRef.current.mouseY;

        if (interaction === 'move') {
          onMove(
            block.id,
            Math.max(0, snap(startRef.current.blockX + dx)),
            Math.max(0, snap(startRef.current.blockY + dy)),
          );
          return;
        }

        const parent = ref.current?.parentElement;
        if (!parent) return;
        const parentW = parent.getBoundingClientRect().width;

        let w = block.width;
        let h = block.height;

        if (interaction === 'resize-right' || interaction === 'resize-corner') {
          w = Math.round(Math.min(100, Math.max(20, (snap(startRef.current.width + dx) / parentW) * 100)));
        }
        if (interaction === 'resize-bottom' || interaction === 'resize-corner') {
          h = Math.min(800, Math.max(150, snap(startRef.current.height + dy)));
        }

        onResize(block.id, w, h);
      });
    };

    const onMouseUp = () => {
      cancelAnimationFrame(rafRef.current);
      setInteraction(null);
    };

    document.body.style.userSelect = 'none';

    if (interaction === 'move') document.body.style.cursor = 'grabbing';
    else if (interaction === 'resize-right') document.body.style.cursor = 'col-resize';
    else if (interaction === 'resize-bottom') document.body.style.cursor = 'row-resize';
    else document.body.style.cursor = 'nwse-resize';

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);

    return () => {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
    }
  }, [interaction, block.id, block.x, block.y, block.width, block.height, onMove, onResize]);

  const widthStyle = block.width >= 100 ? '100%' : `calc(${block.width}% - 4px)`;

  return (
    <div
      ref={ref}
      className={`${styles.container} ${interaction ? styles.interacting : ''}`}
      style={{ left: block.x, top: block.y, width: widthStyle, height: block.height }}
    >
      <div className={styles.header} onMouseDown={startDrag}>
        <span className={styles.title}>{BLOCK_LABELS[block.type]}</span>
        <div className={styles.actions}>
          <button
            type="button"
            className={styles.actionBtn}
            onClick={() => onDelete(block.id)}
            title="Remove block"
          >
            ✕
          </button>
        </div>
      </div>

      <div className={styles.body}>{children}</div>

      <div className={styles.resizeRight} onMouseDown={e => startResize(e, 'resize-right')} />
      <div className={styles.resizeBottom} onMouseDown={e => startResize(e, 'resize-bottom')} />
      <div className={styles.resizeCorner} onMouseDown={e => startResize(e, 'resize-corner')} />
    </div>
  )
}
