export type ContextMenuAction = {
  label: string;
  disabled?: boolean;
  onSelect: () => void;
};

export type ContextMenuPosition = {
  x: number;
  y: number;
};

export type ContextMenu = {
  element: HTMLDivElement;
  open: (
    actions: ContextMenuAction[],
    position: ContextMenuPosition,
    trigger?: HTMLElement,
  ) => void;
  close: (restoreFocus?: boolean) => void;
  destroy: () => void;
};

const MENU_WIDTH = 220;
const MENU_ITEM_HEIGHT = 36;
const VIEWPORT_MARGIN = 8;

export function createContextMenu(): ContextMenu {
  const element = document.createElement("div");
  element.className = "context-menu";
  element.setAttribute("role", "menu");
  element.setAttribute("aria-label", "ファイル操作");
  element.hidden = true;

  let trigger: HTMLElement | undefined;

  const close = (restoreFocus = false) => {
    if (element.hidden) return;
    element.hidden = true;
    trigger?.setAttribute("aria-expanded", "false");
    if (restoreFocus) trigger?.focus();
    trigger = undefined;
  };

  const handlePointerDown = (event: PointerEvent) => {
    if (element.hidden) return;
    const target = event.target;
    if (
      target instanceof Node &&
      !element.contains(target) &&
      target !== trigger
    ) {
      close();
    }
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if (element.hidden) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close(true);
      return;
    }

    const items = Array.from(
      element.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'),
    );
    if (items.length === 0) return;
    const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
    let nextIndex: number | undefined;
    if (event.key === "ArrowDown") nextIndex = (currentIndex + 1) % items.length;
    else if (event.key === "ArrowUp") {
      nextIndex = (currentIndex - 1 + items.length) % items.length;
    } else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = items.length - 1;
    if (nextIndex !== undefined) {
      event.preventDefault();
      items[nextIndex].focus();
    }
  };

  document.addEventListener("pointerdown", handlePointerDown);
  document.addEventListener("keydown", handleKeyDown);

  return {
    element,
    open(actions, position, nextTrigger) {
      trigger?.setAttribute("aria-expanded", "false");
      trigger = nextTrigger;
      trigger?.setAttribute("aria-haspopup", "menu");
      trigger?.setAttribute("aria-expanded", "true");
      element.replaceChildren();

      for (const action of actions) {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "context-menu-item";
        item.setAttribute("role", "menuitem");
        item.textContent = action.label;
        item.disabled = action.disabled ?? false;
        item.addEventListener("click", () => {
          if (item.disabled) return;
          close();
          action.onSelect();
        });
        element.append(item);
      }

      const viewportWidth = Math.max(window.innerWidth, VIEWPORT_MARGIN * 2);
      const viewportHeight = Math.max(window.innerHeight, VIEWPORT_MARGIN * 2);
      const width = Math.min(MENU_WIDTH, viewportWidth - VIEWPORT_MARGIN * 2);
      const height = Math.min(
        actions.length * MENU_ITEM_HEIGHT + VIEWPORT_MARGIN * 2,
        viewportHeight - VIEWPORT_MARGIN * 2,
      );
      const left = Math.max(
        VIEWPORT_MARGIN,
        Math.min(position.x, viewportWidth - width - VIEWPORT_MARGIN),
      );
      const top = Math.max(
        VIEWPORT_MARGIN,
        Math.min(position.y, viewportHeight - height - VIEWPORT_MARGIN),
      );
      element.style.left = `${left}px`;
      element.style.top = `${top}px`;
      element.hidden = false;
      element
        .querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')
        ?.focus();
    },
    close,
    destroy() {
      close();
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      element.remove();
    },
  };
}
