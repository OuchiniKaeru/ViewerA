export type FileDetailsData = {
  name: string;
  size: string;
  type: string;
  mimeType: string;
  modifiedAt: string;
  relativePath: string;
  rootLabel: string;
};

const DETAIL_ROWS: [keyof FileDetailsData, string][] = [
  ["name", "名前"],
  ["size", "サイズ"],
  ["type", "種類"],
  ["mimeType", "MIMEタイプ"],
  ["modifiedAt", "更新日時"],
  ["relativePath", "相対パス"],
  ["rootLabel", "保存先"],
];

export function createFileDetailsDialog(data: FileDetailsData): HTMLDialogElement {
  const dialog = document.createElement("dialog");
  dialog.className = "file-details-dialog";
  dialog.setAttribute("aria-labelledby", "fileDetailsTitle");

  const header = document.createElement("header");
  header.className = "file-details-header";
  const title = document.createElement("h2");
  title.id = "fileDetailsTitle";
  title.textContent = "ファイル詳細";
  const closeButton = document.createElement("button");
  closeButton.type = "button";
  closeButton.className = "icon-button";
  closeButton.setAttribute("aria-label", "詳細を閉じる");
  closeButton.dataset.dialogClose = "true";
  closeButton.textContent = "×";
  closeButton.addEventListener("click", () => {
    dialog.close?.();
    dialog.remove();
  });
  header.append(title, closeButton);

  const list = document.createElement("dl");
  list.className = "file-detail-list";
  for (const [key, label] of DETAIL_ROWS) {
    const term = document.createElement("dt");
    const description = document.createElement("dd");
    term.textContent = label;
    description.textContent = data[key];
    list.append(term, description);
  }

  dialog.addEventListener("close", () => dialog.remove(), { once: true });
  dialog.append(header, list);
  return dialog;
}
