import { useEffect, useLayoutEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  onSelect?: () => void;
  disabled?: boolean;
  checked?: boolean;
  danger?: boolean;
  /** Shows a submenu on hover. */
  items?: MenuItem[];
}

export type MenuEntry = MenuItem | "separator";

export interface MenuState {
  x: number;
  y: number;
  items: MenuEntry[];
}

/** A right-click menu at a screen position, closed by Escape or a click outside. */
export function ContextMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  useEffect(() => {
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      onClose();
    };
    window.addEventListener("keydown", close);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("keydown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  }, [onClose]);

  return (
    <div
      className="menu-backdrop"
      onMouseDown={onClose}
      onContextMenu={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <MenuList items={menu.items} x={menu.x} y={menu.y} onClose={onClose} />
    </div>
  );
}

function MenuList({
  items,
  x,
  y,
  onClose,
}: {
  items: MenuEntry[];
  x: number;
  y: number;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });
  const [open, setOpen] = useState<number | null>(null);

  // Keep the menu on screen.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPosition({
      left: Math.max(4, Math.min(x, window.innerWidth - width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - height - 4)),
    });
  }, [x, y]);

  return (
    <div
      ref={ref}
      className="menu"
      role="menu"
      style={position}
      onMouseDown={(e) => e.stopPropagation()}
    >
      {items.map((item, i) =>
        item === "separator" ? (
          <div key={i} className="menu-separator" role="separator" />
        ) : (
          <div key={i} className="menu-item-wrap" onMouseEnter={() => setOpen(item.items ? i : null)}>
            <button
              role="menuitem"
              className={`menu-item${item.danger ? " danger" : ""}`}
              disabled={item.disabled}
              aria-haspopup={item.items ? "menu" : undefined}
              onClick={() => {
                if (item.items) {
                  setOpen(i);
                  return;
                }
                item.onSelect?.();
                onClose();
              }}
            >
              <span className="menu-check">{item.checked ? "✓" : ""}</span>
              <span className="menu-label">{item.label}</span>
              {item.items && <span className="menu-arrow">›</span>}
            </button>
            {item.items && open === i && <Submenu parent={ref} index={i} items={item.items} onClose={onClose} />}
          </div>
        ),
      )}
    </div>
  );
}

function Submenu({
  parent,
  index,
  items,
  onClose,
}: {
  parent: React.RefObject<HTMLDivElement | null>;
  index: number;
  items: MenuEntry[];
  onClose: () => void;
}) {
  const row = parent.current?.querySelectorAll(".menu-item-wrap")[index]?.getBoundingClientRect();
  const box = parent.current?.getBoundingClientRect();
  if (!row || !box) return null;
  return <MenuList items={items} x={box.right - 2} y={row.top - 4} onClose={onClose} />;
}
