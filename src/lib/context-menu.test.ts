// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { createContextMenu } from "./context-menu";

describe("createContextMenu", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  it("renders accessible actions, executes one, and closes the menu", () => {
    const onSelect = vi.fn();
    const menu = createContextMenu();
    document.body.append(menu.element);

    menu.open(
      [{ label: "ファイル詳細", onSelect }],
      { x: 120, y: 80 },
    );

    const action = document.querySelector<HTMLElement>('[role="menuitem"]');
    expect(action?.textContent).toBe("ファイル詳細");
    action?.click();

    expect(onSelect).toHaveBeenCalledOnce();
    expect(menu.element.hidden).toBe(true);
  });

  it("keeps the menu inside the visible viewport", () => {
    const menu = createContextMenu();
    document.body.append(menu.element);
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 300 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 200 });

    menu.open([{ label: "詳細", onSelect: vi.fn() }], { x: 290, y: 190 });

    expect(Number.parseFloat(menu.element.style.left)).toBeLessThan(290);
    expect(Number.parseFloat(menu.element.style.top)).toBeLessThan(190);
  });

  it("closes on Escape and restores focus to its trigger", () => {
    const menu = createContextMenu();
    const trigger = document.createElement("button");
    document.body.append(trigger, menu.element);
    trigger.focus();
    menu.open([{ label: "詳細", onSelect: vi.fn() }], { x: 20, y: 20 }, trigger);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(menu.element.hidden).toBe(true);
    expect(document.activeElement).toBe(trigger);
  });
});
