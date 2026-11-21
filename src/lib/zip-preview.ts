export type ZipEntry = {
  name: string;
  compressedSize: number;
  uncompressedSize: number;
  isDir: boolean;
};

export function renderZipEntries(
  container: HTMLElement,
  entries: ZipEntry[],
  formatSize: (size: number) => string,
) {
  container.replaceChildren();
  if (entries.length === 0) {
    const empty = document.createElement("div");
    empty.className = "preview-empty";
    empty.textContent = "ZIP内にファイルがありません。";
    container.append(empty);
    return;
  }

  const table = document.createElement("table");
  table.className = "csv-table zip-table";
  const header = document.createElement("thead");
  const headerRow = document.createElement("tr");
  for (const label of ["名前", "圧縮後", "展開後"]) {
    const cell = document.createElement("th");
    cell.textContent = label;
    headerRow.append(cell);
  }
  header.append(headerRow);

  const body = document.createElement("tbody");
  for (const entry of entries) {
    const row = document.createElement("tr");
    const name = document.createElement("td");
    name.textContent = entry.name;
    const compressedSize = document.createElement("td");
    compressedSize.textContent = formatSize(entry.compressedSize);
    const uncompressedSize = document.createElement("td");
    uncompressedSize.textContent = formatSize(entry.uncompressedSize);
    row.append(name, compressedSize, uncompressedSize);
    body.append(row);
  }
  table.append(header, body);
  const scroll = document.createElement("div");
  scroll.className = "csv-scroll";
  scroll.append(table);
  container.append(scroll);
}
