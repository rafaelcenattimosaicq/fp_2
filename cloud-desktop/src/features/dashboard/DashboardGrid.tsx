import { useState, useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import type { DashboardBlock, BlockType } from './types';
import { DEFAULT_BLOCKS, STORAGE_KEY, generateBlockId, snap } from './types';
import { BlockContainer } from './BlockContainer';
import { Block } from './Block';
import { AddBlockBtn } from './AddBlockBtn';
import styles from './DashboardGrid.module.css';

const GAP = 8;


function reflowBlocks(blocks: DashboardBlock[], containerW: number): DashboardBlock[] {
  let cx = 0;
  let cy = 0;
  let rowH = 0;

  return blocks.map(b => {
    const bw = (b.width / 100) * containerW;
    if (cx > 0 && cx + bw > containerW) {
      cx = 0;
      cy += rowH + GAP;
      rowH = 0;
    }
    const out = { ...b, x: snap(cx), y: snap(cy) };
    cx += bw + GAP;
    rowH = Math.max(rowH, b.height);
    return out;
  });
}

function loadBlocks(): DashboardBlock[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const arr = JSON.parse(raw) as Record<string, unknown>[];
      if (Array.isArray(arr) && arr.length > 0) {
        return arr.map(item => ({
          id: String(item.id ?? generateBlockId()),
          type: item.type as BlockType,
          width: typeof item.width === 'number' ? item.width : (item.size === 'two' ? 100 : 50),
          height: typeof item.height === 'number' ? item.height : 300,
          x: typeof item.x === 'number' ? item.x : 0,
          y: typeof item.y === 'number' ? item.y : 0,
        }));
      }
    }
  // eslint-disable-next-line no-empty
  } catch {

  }
  return [...DEFAULT_BLOCKS];
}

function saveBlocks(blocks: DashboardBlock[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(blocks));
}


export function DashboardGrid(): React.JSX.Element {
  const [blocks, setBlocks] = useState<DashboardBlock[]>(loadBlocks);
  const gridRef = useRef<HTMLDivElement>(null);
  const lastW = useRef(0);

  useLayoutEffect(() => {
    const el = gridRef.current;
    if (!el) return;
    const w = el.getBoundingClientRect().width;
    lastW.current = w;

    setBlocks(prev => {
      const allOrigin = prev.every(b => b.x === 0 && b.y === 0);
      return allOrigin ? reflowBlocks(prev, w) : prev;
    });
  }, []);

  // blocks when the scrollbar appears/disappears (which used to trigger at 4px
  useEffect(() => {
    const el = gridRef.current;
    if (!el) return;

    let timer: ReturnType<typeof setTimeout> | null = null;

    const observer = new ResizeObserver(entries => {
      const entry = entries[0];
      if (!entry) return;
      const newW = entry.contentRect.width;

      if (Math.abs(newW - lastW.current) <= 12) return;
      lastW.current = newW;

      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        setBlocks(prev => reflowBlocks(prev, newW));
      }, 150);
    });

    observer.observe(el);
    return () => {
      observer.disconnect();
      if (timer) clearTimeout(timer);
    };
  }, []);

  useEffect(() => { saveBlocks(blocks); }, [blocks]);

  // FIXME: drag on a 7" touchscreen is still a bit janky, the touch events

  const handleMove = useCallback((id: string, x: number, y: number) => {
    setBlocks(prev => prev.map(b => b.id === id ? { ...b, x, y } : b));
  }, []);

  const handleResize = useCallback((id: string, w: number, h: number) => {
    setBlocks(prev => prev.map(b => b.id === id ? { ...b, width: w, height: h } : b));
  }, []);

  function handleDelete(id: string): void {
    setBlocks(prev => prev.filter(b => b.id !== id));
  }

  const handleAdd = useCallback((type: BlockType) => {
    setBlocks(prev => {
      const maxBot = prev.reduce((m, b) => Math.max(m, b.y + b.height), 0);
      const newBlk: DashboardBlock = {
        id: generateBlockId(),
        type,
        width: 50,
        height: 300,
        x: 0,
        y: snap(maxBot + GAP),
      };
      return [...prev, newBlk];
    });
  }, []);

  const canvasH = blocks.reduce((m, b) => Math.max(m, b.y + b.height + 20), 200);

  return (
    <div className={styles.grid} ref={gridRef}>
      <div className={styles.canvas} style={{ minHeight: canvasH }}>
        {blocks.map(blk => (
          <BlockContainer
            key={blk.id}
            block={blk}
            onMove={handleMove}
            onResize={handleResize}
            onDelete={handleDelete}
          >
            <Block type={blk.type} />
          </BlockContainer>
        ))}
      </div>
      <AddBlockBtn onAdd={handleAdd} />
    </div>
  );
}
