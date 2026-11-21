export type FileReference = {
  rootId: string;
  relativePath: string;
  timestamp: string;
};

export function renderFileReferences(
  container: HTMLElement,
  references: FileReference[],
  emptyMessage: string,
  onSelect: (reference: FileReference) => void,
) {
  container.replaceChildren();
  if (references.length === 0) {
    const empty = document.createElement("p");
    empty.className = "saved-reference-empty";
    empty.textContent = emptyMessage;
    container.append(empty);
    return;
  }

  for (const reference of references) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "saved-reference-item";
    button.textContent = reference.relativePath.split("/").filter(Boolean).at(-1) || "ルート";
    button.title = reference.relativePath || "ルート";
    button.addEventListener("click", () => onSelect(reference));
    container.append(button);
  }
}
