import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openPath } from "@tauri-apps/plugin-opener";
import DOMPurify from "dompurify";
import hljs from "highlight.js";
import { marked } from "marked";
import { createContextMenu, type ContextMenuAction } from "./lib/context-menu";
import { pageContentSearchHits } from "./lib/content-search";
import { createFileDetailsDialog } from "./lib/file-details";
import { createMediaPreview } from "./lib/media-preview";
import {
  createHtmlView,
  prepareHtmlForPreview,
} from "./lib/html-view";
import { inlineMhtmlResources, parseMhtml } from "./lib/mhtml";
import { invoke } from "./lib/tauri-runtime";
import {
  renderFileReferences,
  type FileReference,
} from "./lib/file-references";
import { renderZipEntries, type ZipEntry } from "./lib/zip-preview";
import type {
  PDFDocumentLoadingTask,
  PDFDocumentProxy,
} from "pdfjs-dist";
import "highlight.js/styles/github.css";
import "./styles.css";
import type { PreviewType } from "./lib/preview-types";

type RootEntry = {
  id: string;
  label: string;
  path: string;
  addedAt: string;
};

type FileEntry = {
  id: string;
  rootId: string;
  relativePath: string;
  name: string;
  extension: string | null;
  mimeType: string;
  size: number;
  modifiedAt: string | null;
  isDir: boolean;
  previewType: PreviewType;
  previewSupported: boolean;
};

type FilePage = {
  items: FileEntry[];
  page: number;
  pageSize: number;
  total: number;
};

type ContentSearchHit = {
  file: FileEntry;
  snippet: string;
};

type ContentSearchResults = {
  items: ContentSearchHit[];
  total: number;
  scannedFiles: number;
  truncated: boolean;
};

type PreviewDescriptor = {
  rootId: string;
  relativePath: string;
  filePath: string;
  previewType: PreviewType;
  mimeType: string;
  size: number;
  supportsRange: boolean;
};

type TextChunk = {
  text: string;
  offset: number;
  totalBytes: number;
  truncated: boolean;
  encoding: string;
};

type TablePage = {
  headers: string[];
  rows: string[][];
  page: number;
  pageSize: number;
  hasMore: boolean;
  truncated: boolean;
};

type WorkbookDescriptor = {
  sheets: {
    id: string;
    name: string;
    startRow: number;
    startColumn: number;
    rowCount: number;
    columnCount: number;
  }[];
};

type WorksheetRange = {
  sheetId: string;
  startRow: number;
  startColumn: number;
  rows: string[][];
  styles: WorksheetCellStyle[];
};

type WorksheetCellStyle = {
  row: number;
  column: number;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  fontColor: string | null;
  fillColor: string | null;
  horizontalAlignment: string | null;
  verticalAlignment: string | null;
  wrapText: boolean;
  borderTop: string | null;
  borderRight: string | null;
  borderBottom: string | null;
  borderLeft: string | null;
};

type ColorTheme =
  | "light"
  | "dark"
  | "lite-pink"
  | "light-orange"
  | "light-green"
  | "light-purple"
  | "dark-green"
  | "dark-pink"
  | "dark-orange"
  | "dark-yellow";

const COLOR_THEMES: { value: ColorTheme; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "lite-pink", label: "Lite-pink" },
  { value: "light-orange", label: "Light-orange" },
  { value: "light-green", label: "Light-green" },
  { value: "light-purple", label: "Light-purple" },
  { value: "dark-green", label: "Dark-green" },
  { value: "dark-pink", label: "Dark-pink" },
  { value: "dark-orange", label: "Dark-orange" },
  { value: "dark-yellow", label: "Dark-yellow" },
];

type Settings = {
  version: number;
  previewPanelWidth: number;
  sidebarWidth: number;
  showHiddenFiles: boolean;
  maxTextPreviewBytes: number;
  maxPreviewCacheBytes: number;
  colorTheme: ColorTheme;
};

type AgentArtifactView = {
  artifactId: string;
  name: string;
  path: string;
  rootId: string | null;
  relativePath: string | null;
  rootPath: string | null;
  needsRoot: boolean;
  exists: boolean;
};

type AgentResultView = {
  requestId: string;
  platform: string;
  agentName: string | null;
  runId: string | null;
  completedAt: string | null;
  action: "none" | "open_artifacts" | "open_first" | "open_folder" | "show_message";
  message: string | null;
  receivedAt: string;
  artifacts: AgentArtifactView[];
};

type AgentArtifactPreview = Omit<FileEntry, "id"> & { id: string };
type AgentFolderLocation = {
  root: RootEntry;
  relativePath: string;
};

type SortKey = "name" | "size" | "modifiedAt" | "type";
type SortDirection = "asc" | "desc";

const PAGE_SIZE = 100;
const OFFICE_FILE_LIMIT = 100 * 1024 * 1024;
const element = <T extends HTMLElement>(id: string) => {
  const found = document.getElementById(id);
  if (!found) throw new Error(`Missing application element: ${id}`);
  return found as T;
};
function requiredElement<T extends HTMLElement>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`Missing application element: ${selector}`);
  return found;
}

const rootList = element<HTMLDivElement>("rootList");
const workspace = requiredElement<HTMLElement>(".workspace");
const sidebarResizeHandle = element<HTMLDivElement>("sidebarResizeHandle");
const previewResizeHandle = element<HTMLDivElement>("previewResizeHandle");
const fileList = element<HTMLDivElement>("fileList");
const emptyState = element<HTMLDivElement>("emptyState");
const breadcrumbs = element<HTMLDivElement>("breadcrumbs");
const previewPanel = element<HTMLElement>("previewPanel");
const previewContent = element<HTMLDivElement>("previewContent");
const previewToolbar = element<HTMLDivElement>("previewToolbar");
const pagination = element<HTMLDivElement>("pagination");
const pageLabel = element<HTMLSpanElement>("pageLabel");
const searchInput = element<HTMLInputElement>("searchInput");
const searchMode = element<HTMLSelectElement>("searchMode");
const toast = element<HTMLDivElement>("toast");
const agentResultsPanel = element<HTMLElement>("agentResults");
const agentResultList = element<HTMLDivElement>("agentResultList");
const favoriteList = element<HTMLDivElement>("favoriteList");
const recentFileList = element<HTMLDivElement>("recentFileList");
const fileContextMenu = createContextMenu();
document.body.append(fileContextMenu.element);

let roots: RootEntry[] = [];
let currentRoot: RootEntry | null = null;
let currentPath = "";
let selectedFile: FileEntry | null = null;
let settings: Settings | null = null;
let favorites: FileReference[] = [];
let recentFiles: FileReference[] = [];
let fileSortKey: SortKey = "name";
let fileSortDirection: SortDirection = "asc";
let contentSearchId: string | null = null;
let contentSearchGeneration = 0;
let contentSearchCounter = 0;
let cachedContentSearch: {
  rootId: string;
  relativePath: string;
  query: string;
  showHiddenFiles: boolean;
  results: ContentSearchResults;
} | null = null;
let page = 0;
let total = 0;
let renderGeneration = 0;
let currentObjectUrl: string | null = null;
let currentPdfTask: PDFDocumentLoadingTask | null = null;
let currentPdf: PDFDocumentProxy | null = null;
let currentPdfPage = 1;
let currentPdfScale = 1;
let toastTimeout: number | undefined;

marked.setOptions({ gfm: true, breaks: false });

async function initialize() {
  try {
    settings = await invoke<Settings>("get_settings");
    roots = await invoke<RootEntry[]>("list_roots");
    setPanelWidth(settings.previewPanelWidth);
    setSidebarWidth(settings.sidebarWidth ?? 220);
    applyTheme(settings.colorTheme ?? "light");
    setupResizeHandles();
    renderRoots();
    await listen<unknown>("agent-return-received", () => void refreshAgentResults());
    await listen<string>("agent-return-error", (event) => notify(event.payload, true));
    await refreshAgentResults();
    await refreshSavedFiles();
    if (roots.length > 0) await activateRoot(roots[0]);
    else showWelcome();
  } catch (error) {
    showError(error);
  }
}

function showWelcome() {
  currentRoot = null;
  fileList.replaceChildren();
  emptyState.hidden = false;
  element<HTMLHeadingElement>("folderTitle").textContent = "フォルダを選択";
  element<HTMLDivElement>("folderLabel").textContent = "ライブラリ";
  breadcrumbs.replaceChildren();
  pagination.hidden = true;
  closePreview();
}

function renderRoots() {
  rootList.replaceChildren();
  for (const root of roots) {
    const row = document.createElement("div");
    row.className = "root-row";
    row.setAttribute("aria-current", root.id === currentRoot?.id ? "page" : "false");

    const open = document.createElement("button");
    open.className = "root-row";
    open.type = "button";
    open.setAttribute("aria-current", root.id === currentRoot?.id ? "page" : "false");
    open.innerHTML =
      '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 6.5h6l2 2H20a1.5 1.5 0 0 1 1.5 1.5v7.5a1.5 1.5 0 0 1-1.5 1.5h-16a1.5 1.5 0 0 1-1.5-1.5v-9a1.5 1.5 0 0 1 1.5-1.5Z"/></svg>';
    const label = document.createElement("span");
    label.textContent = root.label;
    open.append(label);
    open.addEventListener("click", () => void activateRoot(root));

    const remove = document.createElement("button");
    remove.className = "root-remove";
    remove.type = "button";
    remove.title = "登録を解除";
    remove.setAttribute("aria-label", `${root.label} の登録を解除`);
    remove.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M10 11v6m4-6v6M7 7l1 13h8l1-13M9 7V4h6v3"/></svg>';
    remove.addEventListener("click", (event) => {
      event.stopPropagation();
      void removeRoot(root);
    });
    row.append(open, remove);
    rootList.append(row);
  }
}

async function refreshAgentResults() {
  try {
    const results = await invoke<AgentResultView[]>("get_agent_results");
    renderAgentResults(results);
  } catch (error) {
    notify(errorMessage(error), true);
  }
}

async function refreshSavedFiles() {
  try {
    const [nextFavorites, nextRecentFiles] = await Promise.all([
      invoke<FileReference[]>("get_favorites"),
      invoke<FileReference[]>("get_recent_files"),
    ]);
    favorites = nextFavorites;
    recentFiles = nextRecentFiles;
    renderFileReferences(
      favoriteList,
      favorites,
      "お気に入りはありません",
      (reference) => void openSavedFileReference(reference),
    );
    renderFileReferences(
      recentFileList,
      recentFiles.slice(0, 6),
      "最近使った項目はありません",
      (reference) => void openSavedFileReference(reference),
    );
  } catch (error) {
    notify(errorMessage(error), true);
  }
}

function isFileFavorite(file: FileEntry) {
  return favorites.some(
    (reference) =>
      reference.rootId === file.rootId &&
      reference.relativePath === file.relativePath,
  );
}

async function toggleFileFavorite(file: FileEntry) {
  try {
    const nextFavorite = !isFileFavorite(file);
    await invoke("set_file_favorite", {
      rootId: file.rootId,
      relativePath: file.relativePath,
      isFavorite: nextFavorite,
    });
    await refreshSavedFiles();
    notify(nextFavorite ? "お気に入りに追加しました。" : "お気に入りから削除しました。");
  } catch (error) {
    showError(error);
  }
}

async function openSavedFileReference(reference: FileReference) {
  try {
    const root = roots.find((candidate) => candidate.id === reference.rootId);
    if (!root) {
      throw new Error("登録済みRootがないため、この項目を開けません。");
    }
    const file = await invoke<FileEntry>("get_file_metadata", {
      rootId: reference.rootId,
      relativePath: reference.relativePath,
    });
    currentRoot = root;
    page = 0;
    renderRoots();
    if (file.isDir) {
      closePreview(false);
      currentPath = file.relativePath;
      await loadDirectory();
      return;
    }
    selectedFile = file;
    currentPath = file.relativePath.split("/").slice(0, -1).join("/");
    await loadDirectory();
    await openPreview(file);
  } catch (error) {
    showError(error);
  }
}

function renderAgentResults(results: AgentResultView[]) {
  agentResultList.replaceChildren();
  agentResultsPanel.hidden = results.length === 0;
  for (const result of results) {
    const card = document.createElement("article");
    card.className = "agent-result";
    const heading = document.createElement("div");
    heading.className = "agent-result-heading";
    const title = document.createElement("h3");
    title.textContent = result.agentName || result.platform;
    const dismiss = document.createElement("button");
    dismiss.className = "button button-secondary";
    dismiss.type = "button";
    dismiss.textContent = "閉じる";
    dismiss.addEventListener("click", () => void dismissAgentResult(result.requestId));
    heading.append(title, dismiss);
    card.append(heading);

    const meta = document.createElement("p");
    meta.className = "agent-result-meta";
    meta.textContent = `${result.platform}${result.runId ? ` · ${result.runId}` : ""} · ${formatDate(result.receivedAt)}`;
    card.append(meta);
    if (result.message) {
      const summary = document.createElement("p");
      summary.className = "agent-result-message";
      summary.textContent = result.message;
      card.append(summary);
    }

    if (result.artifacts.length > 0) {
      const artifacts = document.createElement("ul");
      artifacts.className = "agent-result-artifacts";
      result.artifacts.forEach((artifact, index) => {
        const row = document.createElement("li");
        const name = document.createElement("span");
        name.textContent = artifact.name + (artifact.exists ? "" : " (見つかりません)");
        const action = document.createElement("button");
        action.className = "button button-secondary";
        action.type = "button";
        const onlyFirst = result.action === "open_first" && index > 0;
        action.textContent = onlyFirst
          ? "選択対象外"
          : artifact.needsRoot
            ? "フォルダを選択"
            : result.action === "open_folder"
              ? "フォルダを表示"
              : result.action === "open_first"
                ? "先頭をプレビュー"
                : "プレビュー";
        action.disabled = !artifact.exists || onlyFirst;
        action.addEventListener("click", () => {
          if (artifact.needsRoot) {
            void selectAgentArtifactRoot(result.requestId);
          } else if (result.action === "open_folder") {
            void openAgentArtifactFolder(result.requestId, artifact);
          } else {
            void openAgentArtifact(result.requestId, artifact);
          }
        });
        row.append(name, action);
        artifacts.append(row);
      });
      card.append(artifacts);
    }
    agentResultList.append(card);
  }
}

async function selectAgentArtifactRoot(requestId: string) {
  try {
    const root = await invoke<RootEntry | null>("select_root_folder");
    if (!root) return;
    roots = await invoke<RootEntry[]>("list_roots");
    renderRoots();
    await invoke<RootEntry>("confirm_agent_result_root", {
      requestId,
      rootId: root.id,
    });
    await refreshAgentResults();
    notify("フォルダを確認しました。成果物をプレビューできます。");
  } catch (error) {
    showError(error);
  }
}

async function openAgentArtifact(requestId: string, artifact: AgentArtifactView) {
  if (!artifact.rootId) {
    notify("先に成果物のフォルダを確認してください。", true);
    return;
  }
  try {
    const file = await invoke<AgentArtifactPreview>("open_agent_artifact", {
      requestId,
      artifactId: artifact.artifactId,
      rootId: artifact.rootId,
    });
    selectedFile = file;
    const renderResult = await openPreview(file);
    if (renderResult === "rendered") {
      await invoke("mark_agent_artifact_opened", {
        requestId,
        artifactId: artifact.artifactId,
      });
    } else if (renderResult === "failed") {
      await invoke("mark_agent_artifact_failed", {
        requestId,
        artifactId: artifact.artifactId,
        error: "The preview renderer could not display this artifact.",
      });
    }
    await refreshAgentResults();
  } catch (error) {
    const message = errorMessage(error);
    try {
      await invoke("mark_agent_artifact_failed", {
        requestId,
        artifactId: artifact.artifactId,
        error: message,
      });
    } catch (receiptError) {
      notify(`${message} (${errorMessage(receiptError)})`, true);
      return;
    }
    showError(error);
  }
}

async function openAgentArtifactFolder(requestId: string, artifact: AgentArtifactView) {
  if (!artifact.rootId) {
    notify("先に成果物のフォルダを確認してください。", true);
    return;
  }
  try {
    const location = await invoke<AgentFolderLocation>("open_agent_artifact_folder", {
      requestId,
      artifactId: artifact.artifactId,
      rootId: artifact.rootId,
    });
    closePreview(false);
    currentRoot = location.root;
    currentPath = location.relativePath;
    page = 0;
    renderRoots();
    await loadDirectory();
  } catch (error) {
    showError(error);
  }
}

async function dismissAgentResult(requestId: string) {
  try {
    await invoke<boolean>("dismiss_agent_result", { requestId });
    await refreshAgentResults();
  } catch (error) {
    showError(error);
  }
}

async function dismissAllAgentResults() {
  try {
    const results = await invoke<AgentResultView[]>("get_agent_results");
    for (const result of results) {
      await invoke<boolean>("dismiss_agent_result", { requestId: result.requestId });
    }
    await refreshAgentResults();
  } catch (error) {
    showError(error);
  }
}

async function activateRoot(root: RootEntry) {
  currentRoot = root;
  currentPath = "";
  page = 0;
  selectedFile = null;
  renderRoots();
  await loadDirectory();
}

async function chooseRoot() {
  try {
    const root = await invoke<RootEntry | null>("select_root_folder");
    if (!root) return;
    roots = await invoke<RootEntry[]>("list_roots");
    renderRoots();
    await activateRoot(root);
  } catch (error) {
    showError(error);
  }
}

async function removeRoot(root: RootEntry) {
  if (!window.confirm(`「${root.label}」の登録を解除しますか？\nフォルダやファイルは削除されません。`)) return;
  try {
    await invoke<boolean>("remove_root", { rootId: root.id });
    roots = await invoke<RootEntry[]>("list_roots");
    if (currentRoot?.id === root.id) {
      currentRoot = null;
      selectedFile = null;
      showWelcome();
    }
    renderRoots();
    notify("フォルダの登録を解除しました。");
  } catch (error) {
    showError(error);
  }
}

function renderSortControls() {
  const toolbarActions = document.querySelector(".toolbar-actions");
  if (!toolbarActions || toolbarActions.querySelector("#fileSortSelect")) return;

  const sortWrapper = document.createElement("div");
  sortWrapper.className = "sort-controls";

  const sortLabel = document.createElement("span");
  sortLabel.className = "sort-label";
  sortLabel.textContent = "並び順";

  const sortSelect = document.createElement("select");
  sortSelect.id = "fileSortSelect";
  sortSelect.className = "file-sort";
  sortSelect.setAttribute("aria-label", "ファイルの並び順");
  sortSelect.innerHTML = `
    <option value="name">名前</option>
    <option value="type">種類</option>
    <option value="size">サイズ</option>
    <option value="modifiedAt">更新日時</option>
  `;
  sortSelect.value = fileSortKey;
  sortSelect.addEventListener("change", () => {
    fileSortKey = sortSelect.value as SortKey;
    page = 0;
    void loadDirectory();
  });

  const directionButton = toolbarButton(fileSortDirection === "asc" ? "↑" : "↓", "並び順を切り替え");
  directionButton.className = "button button-secondary sort-direction";
  directionButton.addEventListener("click", () => {
    fileSortDirection = fileSortDirection === "asc" ? "desc" : "asc";
    directionButton.textContent = fileSortDirection === "asc" ? "↑" : "↓";
    directionButton.setAttribute("aria-label", fileSortDirection === "asc" ? "昇順" : "降順");
    page = 0;
    void loadDirectory();
  });

  sortWrapper.append(sortLabel, sortSelect, directionButton);
  toolbarActions.insertBefore(sortWrapper, toolbarActions.lastElementChild);
}

function cancelActiveContentSearch() {
  const searchId = contentSearchId;
  contentSearchId = null;
  contentSearchGeneration += 1;
  if (searchId) {
    void invoke<boolean>("cancel_content_search", { searchId }).catch(showError);
  }
}

async function searchContent() {
  if (!currentRoot) return;
  cancelActiveContentSearch();
  const generation = ++contentSearchGeneration;
  const searchId = `search-${Date.now()}-${++contentSearchCounter}`;
  contentSearchId = searchId;
  const rootId = currentRoot.id;
  const relativePath = currentPath;
  const query = searchInput.value.trim();
  const showHiddenFiles = settings?.showHiddenFiles ?? false;
  fileList.replaceChildren(message("内容を検索しています…"));
  pagination.hidden = true;
  pageLabel.textContent = "検索中…";
  try {
    const result = await invoke<ContentSearchResults>("search_file_contents", {
      rootId,
      relativePath,
      query,
      searchId,
    });
    if (generation !== contentSearchGeneration) return;
    contentSearchId = null;
    cachedContentSearch = {
      rootId,
      relativePath,
      query,
      showHiddenFiles,
      results: result,
    };
    renderContentSearchPage(cachedContentSearch);
  } catch (error) {
    if (generation === contentSearchGeneration) {
      contentSearchId = null;
      showError(error);
      fileList.replaceChildren();
      emptyState.hidden = true;
      pagination.hidden = true;
      const errorCard = document.createElement("div");
      errorCard.className = "error-card";
      errorCard.textContent = errorMessage(error);
      fileList.append(errorCard);
    }
  }
}

function renderContentSearchPage(search: NonNullable<typeof cachedContentSearch>) {
  const results = search.results;
  const searchPage = pageContentSearchHits(results.items, page, PAGE_SIZE);
  page = searchPage.page;
  total = searchPage.total;
  const hits = searchPage.items;
  const files = hits.map((hit) => hit.file);
  const snippets = new Map(hits.map((hit) => [hit.file.id, hit.snippet]));
  renderFiles(files, snippets);
  updatePagination({
    ...searchPage,
    items: files,
  });
  if (results.truncated) {
    pageLabel.textContent += " · 検索範囲の上限に達しました";
  }
}

async function loadDirectory() {
  if (!currentRoot) {
    cancelActiveContentSearch();
    showWelcome();
    return;
  }
  const title = currentPath.split("/").filter(Boolean).at(-1) || currentRoot.label;
  element<HTMLHeadingElement>("folderTitle").textContent = title;
  element<HTMLDivElement>("folderLabel").textContent = currentRoot.label;
  renderBreadcrumbs();
  renderSortControls();
  if (searchMode.value === "content" && searchInput.value.trim()) {
    if (
      cachedContentSearch?.rootId === currentRoot.id &&
      cachedContentSearch.relativePath === currentPath &&
      cachedContentSearch.query === searchInput.value.trim() &&
      cachedContentSearch.showHiddenFiles === (settings?.showHiddenFiles ?? false)
    ) {
      cancelActiveContentSearch();
      renderContentSearchPage(cachedContentSearch);
      return;
    }
    await searchContent();
    return;
  }
  cancelActiveContentSearch();

  try {
    const result = await invoke<FilePage>("list_directory", {
      rootId: currentRoot.id,
      relativePath: currentPath,
      page,
      pageSize: PAGE_SIZE,
      query: searchMode.value === "name" ? searchInput.value.trim() : "",
      sortBy: fileSortKey,
      sortOrder: fileSortDirection,
    });
    total = result.total;
    renderFiles(result.items);
    updatePagination(result);
  } catch (error) {
    showError(error);
  }
}

function renderBreadcrumbs() {
  breadcrumbs.replaceChildren();
  if (!currentRoot) return;
  const segments = currentPath.split("/").filter(Boolean);
  const rootButton = document.createElement("button");
  rootButton.type = "button";
  rootButton.textContent = currentRoot.label;
  rootButton.addEventListener("click", () => {
    currentPath = "";
    page = 0;
    void loadDirectory();
  });
  breadcrumbs.append(rootButton);
  let accumulated = "";
  for (const segment of segments) {
    const separator = document.createElement("span");
    separator.textContent = "/";
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = segment;
    accumulated = accumulated ? `${accumulated}/${segment}` : segment;
    const target = accumulated;
    button.addEventListener("click", () => {
      currentPath = target;
      page = 0;
      void loadDirectory();
    });
    breadcrumbs.append(separator, button);
  }
}

function renderFiles(files: FileEntry[], snippets = new Map<string, string>()) {
  fileList.replaceChildren();
  emptyState.hidden = files.length > 0 || !currentRoot;
  if (currentRoot && files.length === 0) {
    element<HTMLHeadingElement>("emptyState").querySelector("h2")!.textContent =
      searchInput.value.trim()
        ? searchMode.value === "content"
          ? "内容が一致するファイルがありません"
          : "一致するファイルがありません"
        : "このフォルダは空です";
    element<HTMLParagraphElement>("emptyState").querySelector("p")!.textContent =
      searchInput.value.trim() ? "検索条件を変更してお試しください。" : "別のフォルダを開くか、ここにファイルを追加してください。";
  }

  for (const file of files) {
    const row = document.createElement("button");
    row.className = "file-row";
    row.type = "button";
    row.setAttribute("role", "listitem");
    row.setAttribute("aria-selected", selectedFile?.id === file.id ? "true" : "false");

    const name = document.createElement("span");
    name.className = "file-name";
    const icon = document.createElement("span");
    icon.className = `file-icon ${file.isDir ? "folder" : file.previewType}`;
    icon.innerHTML = file.isDir
      ? '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 6.5h6l2 2H20a1.5 1.5 0 0 1 1.5 1.5v7.5a1.5 1.5 0 0 1-1.5 1.5h-16a1.5 1.5 0 0 1-1.5-1.5v-9a1.5 1.5 0 0 1 1.5-1.5Z"/></svg>'
      : iconFor(file.previewType);
    const text = document.createElement("span");
    text.className = "file-name-text";
    text.textContent = file.name;
    const snippetText = snippets.get(file.id);
    if (snippetText !== undefined) {
      row.classList.add("has-search-snippet");
      const fileText = document.createElement("span");
      fileText.className = "file-name-content";
      const snippet = document.createElement("span");
      snippet.className = "search-snippet";
      snippet.textContent = snippetText;
      fileText.append(text, snippet);
      name.append(icon, fileText);
    } else {
      name.append(icon, text);
    }

    const type = document.createElement("span");
    type.className = "file-meta";
    type.textContent = file.isDir ? "フォルダ" : file.extension?.toUpperCase() ?? "ファイル";
    const size = document.createElement("span");
    size.className = "file-meta";
    size.textContent = file.isDir ? "—" : formatSize(file.size);
    const modified = document.createElement("span");
    modified.className = "file-meta";
    modified.textContent = formatDate(file.modifiedAt);
    row.append(name, type, size, modified);
    row.addEventListener("click", () => {
      if (file.isDir) {
        currentPath = file.relativePath;
        selectedFile = null;
        page = 0;
        void loadDirectory();
      } else {
        selectedFile = file;
        renderFiles(files);
        void openPreview(file);
      }
    });
    row.addEventListener("dblclick", () => {
      if (!file.isDir) void openPreview(file);
    });
    if (!file.isDir) {
      row.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        openFileContextMenu(file, { x: event.clientX, y: event.clientY }, row);
      });
      row.addEventListener("keydown", (event) => {
        if ((event.shiftKey && event.key === "F10") || event.key === "ContextMenu") {
          event.preventDefault();
          const bounds = row.getBoundingClientRect();
          openFileContextMenu(file, { x: bounds.left, y: bounds.bottom }, row);
        }
      });
    }
    fileList.append(row);
  }
}

function iconFor(type: PreviewType): string {
  const color =
    type === "pdf" ? "M7 3h7l4 4v14H7zM14 3v5h5M9 15h6M9 18h4" :
    type === "image" || type === "svg" ? "M4 5h16v14H4zM4 15l4-4 4 4 3-3 5 5M15 9h.01" :
    type === "spreadsheet" || type === "csv" ? "M4 4h16v16H4zM4 10h16M10 4v16M15 4v16M4 15h16" :
    type === "pptx" ? "M5 4h14v16H5zM9 8h6M9 12h6M9 16h3" :
    type === "docx" ? "M6 3h9l4 4v14H6zM15 3v5h4M9 12h7M9 16h7" :
    "M6 3h9l4 4v14H6zM15 3v5h4M9 12h7M9 16h7";
  return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="${color}"/></svg>`;
}

function updatePagination(result: FilePage) {
  pagination.hidden = result.total <= result.pageSize;
  pageLabel.textContent = `${result.total === 0 ? 0 : result.page * result.pageSize + 1}–${Math.min((result.page + 1) * result.pageSize, result.total)} / ${result.total}`;
  element<HTMLButtonElement>("previousPageButton").disabled = page === 0;
  element<HTMLButtonElement>("nextPageButton").disabled =
    (page + 1) * result.pageSize >= result.total;
}

async function selectedFileDescriptor(file = selectedFile): Promise<PreviewDescriptor> {
  if (!file) {
    throw new Error("開くファイルを選択してください。");
  }
  const descriptor = await invoke<PreviewDescriptor>("get_preview_descriptor", {
    rootId: file.rootId,
    relativePath: file.relativePath,
  });
  if (!descriptor.filePath || descriptor.filePath.trim() === "") {
    throw new Error("対象ファイルのパスが取得できませんでした。");
  }
  return descriptor;
}

async function openSelectedFileExternally(file = selectedFile) {
  try {
    const descriptor = await selectedFileDescriptor(file);
    await openPath(descriptor.filePath);
    notify("外部アプリで開きます。");
  } catch (error) {
    showError(error);
  }
}

async function copySelectedFilePath(file = selectedFile) {
  try {
    const descriptor = await selectedFileDescriptor(file);
    if (!navigator.clipboard) {
      throw new Error("この環境ではクリップボードを利用できません。");
    }
    await navigator.clipboard.writeText(descriptor.filePath);
    notify("ファイルのパスをコピーしました。");
  } catch (error) {
    showError(error);
  }
}

async function showFileDetails(file: FileEntry) {
  try {
    const metadata = await invoke<FileEntry>("get_file_metadata", {
      rootId: file.rootId,
      relativePath: file.relativePath,
    });
    const rootLabel = roots.find((root) => root.id === file.rootId)?.label ?? "未登録";
    const dialog = createFileDetailsDialog({
      name: metadata.name,
      size: formatSize(metadata.size),
      type: metadata.extension?.toUpperCase() ?? metadata.previewType,
      mimeType: metadata.mimeType,
      modifiedAt: formatDate(metadata.modifiedAt),
      relativePath: metadata.relativePath || "ルート直下",
      rootLabel,
    });
    document.body.append(dialog);
    try {
      dialog.showModal();
    } catch (error) {
      dialog.remove();
      throw error;
    }
  } catch (error) {
    showError(error);
  }
}

function fileActions(file: FileEntry): ContextMenuAction[] {
  return [
    { label: "プレビューを開く", onSelect: () => void openPreview(file) },
    { label: "詳細を表示", onSelect: () => void showFileDetails(file) },
    {
      label: isFileFavorite(file) ? "お気に入りから削除" : "お気に入りに追加",
      onSelect: () => void toggleFileFavorite(file),
    },
    { label: "パスをコピー", onSelect: () => void copySelectedFilePath(file) },
    { label: "外部アプリで開く", onSelect: () => void openSelectedFileExternally(file) },
    { label: "親フォルダーを表示", onSelect: () => openFileParentFolder(file) },
  ];
}

function openFileContextMenu(
  file: FileEntry,
  position: { x: number; y: number },
  trigger?: HTMLElement,
) {
  fileContextMenu.open(fileActions(file), position, trigger);
}

function openFileParentFolder(file: FileEntry) {
  const root = roots.find((candidate) => candidate.id === file.rootId);
  if (!root) {
    notify("ファイルのRootが登録されていません。", true);
    return;
  }
  closePreview(false);
  currentRoot = root;
  currentPath = file.relativePath.split("/").slice(0, -1).join("/");
  page = 0;
  renderRoots();
  void loadDirectory();
}

function appendPreviewActions() {
  if (!selectedFile) return;
  const file = selectedFile;
  const menuButton = document.createElement("button");
  menuButton.type = "button";
  menuButton.className = "button button-secondary file-actions-button";
  menuButton.textContent = "その他";
  menuButton.setAttribute("aria-label", `${file.name} の操作`);
  menuButton.addEventListener("click", () => {
    const bounds = menuButton.getBoundingClientRect();
    openFileContextMenu(file, { x: bounds.left, y: bounds.bottom }, menuButton);
  });
  previewToolbar.append(menuButton);
}

async function openPreview(
  file: FileEntry,
): Promise<"rendered" | "failed" | "cancelled"> {
  const generation = ++renderGeneration;
  selectedFile = file;
  workspace.classList.remove("preview-closed");
  previewPanel.hidden = false;
  element<HTMLHeadingElement>("previewTitle").textContent = file.name;
  element<HTMLSpanElement>("previewSubtitle").textContent =
    `${file.extension?.toUpperCase() ?? "FILE"} · ${formatSize(file.size)}`;
  element<HTMLDivElement>("previewFileIcon").textContent =
    file.extension?.toUpperCase().slice(0, 4) ?? "FILE";
  previewToolbar.replaceChildren();
  previewContent.replaceChildren(message("読み込み中…"));
  try {
    const descriptor = await invoke<PreviewDescriptor>("get_preview_descriptor", {
      rootId: file.rootId,
      relativePath: file.relativePath,
    });
    if (generation !== renderGeneration) return "cancelled";
    await renderPreview(descriptor, generation);
    if (generation !== renderGeneration) return "cancelled";
    appendPreviewActions();
    try {
      await invoke("record_recent_file", {
        rootId: file.rootId,
        relativePath: file.relativePath,
      });
      await refreshSavedFiles();
    } catch (error) {
      notify(errorMessage(error), true);
    }
    return generation === renderGeneration ? "rendered" : "cancelled";
  } catch (error) {
    if (generation === renderGeneration) renderError(error);
    return generation === renderGeneration ? "failed" : "cancelled";
  }
}

async function renderPreview(descriptor: PreviewDescriptor, generation: number) {
  previewContent.replaceChildren();
  revokeObjectUrl();
  void currentPdfTask?.destroy();
  currentPdfTask = null;
  currentPdf = null;
  switch (descriptor.previewType) {
    case "image":
    case "svg": {
      const image = document.createElement("img");
      image.className = "preview-image";
      image.alt = selectedFile?.name ?? "Preview image";
      image.src = convertFileSrc(descriptor.filePath);
      previewContent.append(image);
      return;
    }
    case "pdf":
      await renderPdf(descriptor, generation);
      return;
    case "audio":
    case "video":
      renderMedia(descriptor, generation);
      return;
    case "zip":
      await renderZipArchive(descriptor, generation);
      return;
    case "text":
    case "json":
    case "yaml":
      await renderText(descriptor, generation);
      return;
    case "markdown":
      await renderMarkdown(descriptor, generation);
      return;
    case "html":
    case "mhtml":
      await renderHtmlDocument(descriptor, generation);
      return;
    case "csv":
      await renderCsv(descriptor, generation, 0);
      return;
    case "spreadsheet":
      await renderSpreadsheet(descriptor, generation);
      return;
    case "docx":
      await renderWord(descriptor, generation);
      return;
    case "pptx":
      await renderPowerPoint(descriptor, generation);
      return;
    default: {
      const unsupported = document.createElement("div");
      unsupported.className = "unsupported";
      const title = document.createElement("strong");
      title.textContent = "この形式はプレビューできません";
      const details = document.createElement("span");
      details.textContent = "ファイルは読み取り専用で保持されています。";
      unsupported.append(title, details);
      previewContent.append(unsupported);
    }
  }
}

function renderMedia(descriptor: PreviewDescriptor, generation: number) {
  const file = selectedFile;
  const media = createMediaPreview(
    descriptor.previewType as "audio" | "video",
    convertFileSrc(descriptor.filePath),
    file?.name ?? "Media preview",
  );
  media.addEventListener(
    "error",
    () => {
      if (generation === renderGeneration) {
        renderError(new Error("この音声・動画形式はWebViewで再生できません。"));
      }
    },
    { once: true },
  );
  previewContent.replaceChildren(media);
}

async function renderZipArchive(
  descriptor: PreviewDescriptor,
  generation: number,
) {
  const entries = await invoke<ZipEntry[]>("list_zip_entries", {
    rootId: descriptor.rootId,
    relativePath: descriptor.relativePath,
  });
  if (generation !== renderGeneration) return;
  renderZipEntries(previewContent, entries, formatSize);
}

async function renderSpreadsheet(
  descriptor: PreviewDescriptor,
  generation: number,
) {
  const workbook = await invoke<WorkbookDescriptor>("read_workbook", {
    rootId: descriptor.rootId,
    relativePath: descriptor.relativePath,
  });
  if (generation !== renderGeneration) return;
  if (workbook.sheets.length === 0) {
    previewContent.append(message("ワークシートがありません。"));
    return;
  }

  let sheet = workbook.sheets[0];
  let rowOffset = 0;
  let columnOffset = 0;
  const sheetPicker = document.createElement("select");
  sheetPicker.setAttribute("aria-label", "ワークシート");
  sheetPicker.className = "sheet-picker";
  for (const candidate of workbook.sheets) {
    const option = document.createElement("option");
    option.value = candidate.id;
    option.textContent = candidate.name;
    sheetPicker.append(option);
  }
  sheetPicker.addEventListener("change", () => {
    sheet = workbook.sheets.find((candidate) => candidate.id === sheetPicker.value) ?? workbook.sheets[0];
    rowOffset = 0;
    columnOffset = 0;
    void drawRange();
  });

  const previousRows = toolbarButton("↑", "前の行");
  const nextRows = toolbarButton("↓", "次の行");
  const previousColumns = toolbarButton("←", "前の列");
  const nextColumns = toolbarButton("→", "次の列");
  const rangeLabel = document.createElement("span");
  previewToolbar.replaceChildren(
    sheetPicker,
    previousColumns,
    rangeLabel,
    nextColumns,
    previousRows,
    nextRows,
  );
  previousRows.addEventListener("click", () => {
    rowOffset = Math.max(0, rowOffset - 100);
    void drawRange();
  });
  nextRows.addEventListener("click", () => {
    rowOffset = Math.min(Math.max(0, sheet.rowCount - 1), rowOffset + 100);
    void drawRange();
  });
  previousColumns.addEventListener("click", () => {
    columnOffset = Math.max(0, columnOffset - 25);
    void drawRange();
  });
  nextColumns.addEventListener("click", () => {
    columnOffset = Math.min(Math.max(0, sheet.columnCount - 1), columnOffset + 25);
    void drawRange();
  });

  async function drawRange() {
    if (generation !== renderGeneration) return;
    const rowCount = Math.min(100, Math.max(0, sheet.rowCount - rowOffset));
    const columnCount = Math.min(25, Math.max(0, sheet.columnCount - columnOffset));
    if (rowCount === 0 || columnCount === 0) {
      previewContent.replaceChildren(message("このワークシートにデータがありません。"));
      rangeLabel.textContent = "空";
      previousRows.disabled = true;
      nextRows.disabled = true;
      previousColumns.disabled = true;
      nextColumns.disabled = true;
      return;
    }

    const result = await invoke<WorksheetRange>("read_worksheet_range", {
      rootId: descriptor.rootId,
      relativePath: descriptor.relativePath,
      sheetId: sheet.id,
      startRow: rowOffset,
      startColumn: columnOffset,
      rowCount,
      columnCount,
    });
    if (generation !== renderGeneration) return;

    const table = document.createElement("table");
    table.className = "csv-table spreadsheet-table";
    const head = document.createElement("thead");
    const header = document.createElement("tr");
    const corner = document.createElement("th");
    corner.textContent = "";
    header.append(corner);
    for (let column = 0; column < columnCount; column += 1) {
      const cell = document.createElement("th");
      cell.textContent = columnLabel(columnOffset + column);
      header.append(cell);
    }
    head.append(header);
    const body = document.createElement("tbody");
    const styles = new Map(
      result.styles.map((style) => [`${style.row}:${style.column}`, style]),
    );
    result.rows.forEach((row, rowIndex) => {
      const tableRow = document.createElement("tr");
      const rowHeader = document.createElement("th");
      rowHeader.textContent = String(rowOffset + rowIndex + 1);
      tableRow.append(rowHeader);
      row.forEach((value, columnIndex) => {
        const cell = document.createElement("td");
        cell.textContent = value;
        const style = styles.get(
          `${rowOffset + rowIndex}:${columnOffset + columnIndex}`,
        );
        if (style) applyWorksheetCellStyle(cell, style);
        tableRow.append(cell);
      });
      body.append(tableRow);
    });
    table.append(head, body);
    const scroll = document.createElement("div");
    scroll.className = "csv-scroll";
    scroll.append(table);
    previewContent.replaceChildren(scroll);
    rangeLabel.textContent =
      `${rowOffset + 1}–${rowOffset + rowCount} 行 · ${columnLabel(columnOffset)}–${columnLabel(columnOffset + columnCount - 1)}`;
    previousRows.disabled = rowOffset === 0;
    nextRows.disabled = rowOffset + rowCount >= sheet.rowCount;
    previousColumns.disabled = columnOffset === 0;
    nextColumns.disabled = columnOffset + columnCount >= sheet.columnCount;
  }

  await drawRange();
}

function applyWorksheetCellStyle(cell: HTMLTableCellElement, style: WorksheetCellStyle) {
  if (style.bold) cell.style.fontWeight = "bold";
  if (style.italic) cell.style.fontStyle = "italic";
  if (style.underline || style.strikethrough) {
    cell.style.textDecoration = [
      style.underline ? "underline" : "",
      style.strikethrough ? "line-through" : "",
    ].filter(Boolean).join(" ");
  }
  if (style.fontColor) cell.style.color = style.fontColor;
  if (style.fillColor) cell.style.backgroundColor = style.fillColor;
  if (style.horizontalAlignment) cell.style.textAlign = style.horizontalAlignment;
  if (style.verticalAlignment) cell.style.verticalAlign = style.verticalAlignment;
  if (style.wrapText) cell.style.whiteSpace = "normal";
  if (style.borderTop) cell.style.borderTop = style.borderTop;
  if (style.borderRight) cell.style.borderRight = style.borderRight;
  if (style.borderBottom) cell.style.borderBottom = style.borderBottom;
  if (style.borderLeft) cell.style.borderLeft = style.borderLeft;
}

function columnLabel(index: number) {
  let value = index + 1;
  let label = "";
  while (value > 0) {
    value -= 1;
    label = String.fromCharCode(65 + (value % 26)) + label;
    value = Math.floor(value / 26);
  }
  return label;
}

async function renderText(descriptor: PreviewDescriptor, generation: number) {
  const chunk = await invoke<TextChunk>("read_text_preview", {
    rootId: descriptor.rootId,
    relativePath: descriptor.relativePath,
    offset: 0,
    limit: settings?.maxTextPreviewBytes ?? 5 * 1024 * 1024,
  });
  if (generation !== renderGeneration) return;
  let value = chunk.text;
  if (descriptor.previewType === "json") {
    try {
      value = JSON.stringify(JSON.parse(value), null, 2);
    } catch {
      notify("JSONを解析できないため、テキストとして表示します。", true);
    }
  }
  const language =
    descriptor.previewType === "json" ? "language-json" :
    descriptor.previewType === "yaml" ? "language-yaml" :
    languageClass(selectedFile?.extension);
  renderSourceCode(value, language);
  if (chunk.truncated) {
    const warning = message(`ファイルが大きいため先頭 ${formatSize(chunk.text.length)} を表示しています。`);
    warning.classList.add("preview-warning");
    previewContent.prepend(warning);
  }
}

function renderSourceCode(value: string, language: string) {
  const code = document.createElement("code");
  code.textContent = value;
  if (language) code.className = language;
  if (code.className) hljs.highlightElement(code);
  const pre = document.createElement("pre");
  pre.append(code);
  previewContent.replaceChildren(pre);
}

type ViewMode = {
  label: string;
  render: () => void | Promise<void>;
};

function addViewToggle(modes: ViewMode[], generation: number) {
  const group = document.createElement("div");
  group.className = "view-toggle";
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", "表示モード");
  const buttons = modes.map((mode, index) => {
    const button = toolbarButton(mode.label, `${mode.label}に切り替え`);
    button.classList.toggle("active", index === 0);
    button.setAttribute("aria-pressed", index === 0 ? "true" : "false");
    button.addEventListener("click", () => {
      if (generation !== renderGeneration) return;
      buttons.forEach((candidate, candidateIndex) => {
        const active = candidateIndex === index;
        candidate.classList.toggle("active", active);
        candidate.setAttribute("aria-pressed", active ? "true" : "false");
      });
      previewContent.replaceChildren();
      void Promise.resolve(mode.render()).catch(renderError);
    });
    group.append(button);
    return button;
  });
  previewToolbar.append(group);
}

async function renderMarkdown(descriptor: PreviewDescriptor, generation: number) {
  const chunk = await invoke<TextChunk>("read_text_preview", {
    rootId: descriptor.rootId,
    relativePath: descriptor.relativePath,
    offset: 0,
    limit: settings?.maxTextPreviewBytes ?? 5 * 1024 * 1024,
  });
  if (generation !== renderGeneration) return;
  const value = chunk.text;
  const showPreview = async () => {
    const html = await marked.parse(value);
    const sanitized = DOMPurify.sanitize(html, {
      FORBID_TAGS: ["img", "svg", "iframe", "script", "style", "video", "audio"],
      FORBID_ATTR: ["style", "href", "src"],
    });
    if (generation !== renderGeneration) return;
    const article = document.createElement("article");
    article.className = "markdown-preview";
    article.innerHTML = sanitized;
    previewContent.replaceChildren(article);
  };
  addViewToggle(
    [
      { label: "プレビュー", render: showPreview },
      { label: "コード", render: () => renderSourceCode(value, "language-markdown") },
    ],
    generation,
  );
  await showPreview();
  if (generation !== renderGeneration) return;
  if (chunk.truncated) {
    const warning = message(`ファイルが大きいため先頭 ${formatSize(chunk.text.length)} を表示しています。`);
    warning.classList.add("preview-warning");
    previewContent.prepend(warning);
  }
}

async function renderHtmlDocument(
  descriptor: PreviewDescriptor,
  generation: number,
) {
  let prepared: string;
  let source: string;
  let truncated = false;
  let truncatedBytes = 0;
  if (descriptor.previewType === "mhtml") {
    const response = await fetch(convertFileSrc(descriptor.filePath));
    if (!response.ok) throw new Error("MHTMLファイルを読み込めませんでした。");
    const buffer = await response.arrayBuffer();
    if (generation !== renderGeneration) return;
    const archive = parseMhtml(buffer);
    prepared = inlineMhtmlResources(archive);
    const limit = settings?.maxTextPreviewBytes ?? 5 * 1024 * 1024;
    truncated = buffer.byteLength > limit;
    truncatedBytes = Math.min(buffer.byteLength, limit);
    source = new TextDecoder("utf-8").decode(buffer.slice(0, limit));
  } else {
    const chunk = await invoke<TextChunk>("read_text_preview", {
      rootId: descriptor.rootId,
      relativePath: descriptor.relativePath,
      offset: 0,
      limit: settings?.maxTextPreviewBytes ?? 5 * 1024 * 1024,
    });
    if (generation !== renderGeneration) return;
    source = chunk.text;
    truncated = chunk.truncated;
    truncatedBytes = chunk.text.length;
    const baseDir = descriptor.filePath.split("/").slice(0, -1).join("/");
    prepared = await prepareHtmlForPreview(source, {
      baseDir,
      toAssetUrl: (path) => convertFileSrc(path),
      fetchText: async (url) => {
        const response = await fetch(url);
        if (!response.ok) throw new Error("スタイルシートを読み込めませんでした。");
        return response.text();
      },
    });
  }
  if (generation !== renderGeneration) return;
  const label = selectedFile?.name ?? "HTML preview";
  const showPreview = () => {
    if (generation !== renderGeneration) return;
    previewContent.replaceChildren(createHtmlView(prepared, label));
  };
  addViewToggle(
    [
      { label: "プレビュー", render: showPreview },
      {
        label: "コード",
        render: () =>
          renderSourceCode(
            source,
            descriptor.previewType === "mhtml" ? "" : "language-html",
          ),
      },
    ],
    generation,
  );
  showPreview();
  if (truncated) {
    const warning = message(`ファイルが大きいため先頭 ${formatSize(truncatedBytes)} を表示しています。`);
    warning.classList.add("preview-warning");
    previewContent.prepend(warning);
  }
}

function languageClass(extension: string | null | undefined) {
  const languageByExtension: Record<string, string> = {
    rs: "language-rust",
    py: "language-python",
    js: "language-javascript",
    jsx: "language-javascript",
    ts: "language-typescript",
    tsx: "language-typescript",
    html: "language-html",
    css: "language-css",
    toml: "language-ini",
    xml: "language-xml",
  };
  return extension ? languageByExtension[extension.toLowerCase()] ?? "" : "";
}

async function renderCsv(
  descriptor: PreviewDescriptor,
  generation: number,
  csvPage: number,
) {
  const result = await invoke<TablePage>("read_csv_preview", {
    rootId: descriptor.rootId,
    relativePath: descriptor.relativePath,
    page: csvPage,
    pageSize: 100,
    hasHeader: true,
  });
  if (generation !== renderGeneration) return;
  const table = document.createElement("table");
  table.className = "csv-table";
  const head = document.createElement("thead");
  const headerRow = document.createElement("tr");
  for (const value of result.headers) {
    const cell = document.createElement("th");
    cell.textContent = value;
    headerRow.append(cell);
  }
  head.append(headerRow);
  const body = document.createElement("tbody");
  for (const row of result.rows) {
    const tableRow = document.createElement("tr");
    for (const value of row) {
      const cell = document.createElement("td");
      cell.textContent = value;
      tableRow.append(cell);
    }
    body.append(tableRow);
  }
  table.append(head, body);
  const scroll = document.createElement("div");
  scroll.className = "csv-scroll";
  scroll.append(table);
  previewContent.replaceChildren(scroll);
  previewToolbar.replaceChildren();
  const previous = toolbarButton("‹", "前のページ");
  previous.disabled = csvPage === 0;
  previous.addEventListener("click", () =>
    void renderCsv(descriptor, generation, csvPage - 1),
  );
  const pageText = document.createElement("span");
  pageText.textContent = `ページ ${csvPage + 1}`;
  const next = toolbarButton("›", "次のページ");
  next.disabled = !result.hasMore;
  next.addEventListener("click", () =>
    void renderCsv(descriptor, generation, csvPage + 1),
  );
  previewToolbar.append(previous, pageText, next);
  if (result.truncated) notify("CSVの表示上限に達しました。", true);
}

async function renderPdf(descriptor: PreviewDescriptor, generation: number) {
  const [{ getDocument, GlobalWorkerOptions }, worker] = await Promise.all([
    import("pdfjs-dist"),
    import("pdfjs-dist/build/pdf.worker.min.mjs?url"),
  ]);
  GlobalWorkerOptions.workerSrc = worker.default;
  const loadingTask = getDocument({ url: convertFileSrc(descriptor.filePath) });
  const pdf = await loadingTask.promise;
  if (generation !== renderGeneration) {
    await loadingTask.destroy();
    return;
  }
  currentPdfTask = loadingTask;
  currentPdf = pdf;
  currentPdfPage = 1;
  currentPdfScale = 1;
  const renderPage = async () => {
    if (!currentPdf || generation !== renderGeneration) return;
    const pdfPage = await currentPdf.getPage(currentPdfPage);
    const viewport = pdfPage.getViewport({ scale: currentPdfScale });
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
    if (!context) throw new Error("PDF canvas is unavailable.");
    canvas.width = viewport.width;
    canvas.height = viewport.height;
    canvas.className = "pdf-page";
    await pdfPage.render({ canvas, viewport }).promise;
    if (generation !== renderGeneration) return;
    previewContent.replaceChildren(canvas);
    updatePdfToolbar(renderPage);
  };
  updatePdfToolbar(renderPage);
  await renderPage();
}

function updatePdfToolbar(renderPage: () => Promise<void>) {
  if (!currentPdf) return;
  previewToolbar.replaceChildren();
  const previous = toolbarButton("‹", "前のページ");
  previous.disabled = currentPdfPage <= 1;
  previous.addEventListener("click", () => {
    if (!currentPdf || currentPdfPage <= 1) return;
    currentPdfPage -= 1;
    void renderPage().catch(showError);
  });
  const pageText = document.createElement("span");
  pageText.textContent = `${currentPdfPage} / ${currentPdf.numPages}`;
  const next = toolbarButton("›", "次のページ");
  next.disabled = currentPdfPage >= currentPdf.numPages;
  next.addEventListener("click", () => {
    if (!currentPdf || currentPdfPage >= currentPdf.numPages) return;
    currentPdfPage += 1;
    void renderPage().catch(showError);
  });
  const zoomOut = toolbarButton("−", "縮小");
  zoomOut.addEventListener("click", () => {
    currentPdfScale = Math.max(0.5, currentPdfScale - 0.1);
    void renderPage().catch(showError);
  });
  const zoomIn = toolbarButton("+", "拡大");
  zoomIn.addEventListener("click", () => {
    currentPdfScale = Math.min(2.5, currentPdfScale + 0.1);
    void renderPage().catch(showError);
  });
  previewToolbar.append(previous, pageText, next, zoomOut, zoomIn);
}

async function renderWord(descriptor: PreviewDescriptor, generation: number) {
  const { renderAsync: renderDocx } = await import("docx-preview");
  if (descriptor.size > OFFICE_FILE_LIMIT) {
    throw new Error("Word文書は100 MiB以下のファイルのみプレビューできます。");
  }
  const response = await fetch(convertFileSrc(descriptor.filePath));
  if (!response.ok) throw new Error("Word文書を読み込めませんでした。");
  const buffer = await response.arrayBuffer();
  if (generation !== renderGeneration) return;
  await renderDocx(buffer, previewContent, previewContent, {
    inWrapper: true,
    renderHeaders: true,
    renderFooters: true,
    renderFootnotes: true,
    renderEndnotes: true,
    renderComments: false,
    useBase64URL: true,
    ignoreLastRenderedPageBreak: false,
  });
}

async function renderPowerPoint(
  descriptor: PreviewDescriptor,
  generation: number,
) {
  const [{ getSlides, loadPresentation }, { renderSlideToSvg }] =
    await Promise.all([
      import("@office-kit/pptx"),
      import("@office-kit/pptx-preview"),
    ]);
  if (descriptor.size > OFFICE_FILE_LIMIT) {
    throw new Error("PowerPoint文書は100 MiB以下のファイルのみプレビューできます。");
  }
  const response = await fetch(convertFileSrc(descriptor.filePath));
  if (!response.ok) throw new Error("PowerPoint文書を読み込めませんでした。");
  const buffer = await response.arrayBuffer();
  if (generation !== renderGeneration) return;
  const presentation = await loadPresentation(
    new Uint8Array(buffer),
    {
      limits: {
        maxEntries: 5000,
        maxTotalUncompressedBytes: 512 * 1024 * 1024,
        maxXmlPartBytes: 32 * 1024 * 1024,
        maxTotalXmlBytes: 256 * 1024 * 1024,
      },
    },
  );
  const slides = getSlides(presentation);
  if (slides.length === 0) {
    throw new Error("PowerPoint文書にスライドがありません。");
  }
  let slideIndex = 0;
  const showSlide = () => {
    if (generation !== renderGeneration) return;
    const svg = renderSlideToSvg(presentation, slides[slideIndex]);
    const sanitizedSvg = DOMPurify.sanitize(svg, {
      USE_PROFILES: { svg: true, svgFilters: true },
      FORBID_TAGS: ["script", "foreignObject", "iframe"],
    });
    revokeObjectUrl();
    currentObjectUrl = URL.createObjectURL(
      new Blob([sanitizedSvg], { type: "image/svg+xml" }),
    );
    const image = document.createElement("img");
    image.className = "preview-image";
    image.alt = `スライド ${slideIndex + 1}`;
    image.src = currentObjectUrl;
    previewContent.replaceChildren(image);
    slideText.textContent = `${slideIndex + 1} / ${slides.length}`;
    previous.disabled = slideIndex === 0;
    next.disabled = slideIndex === slides.length - 1;
  };
  previewToolbar.replaceChildren();
  const previous = toolbarButton("‹", "前のスライド");
  const slideText = document.createElement("span");
  const next = toolbarButton("›", "次のスライド");
  previous.addEventListener("click", () => {
    slideIndex = Math.max(0, slideIndex - 1);
    showSlide();
  });
  next.addEventListener("click", () => {
    slideIndex = Math.min(slides.length - 1, slideIndex + 1);
    showSlide();
  });
  previewToolbar.append(previous, slideText, next);
  showSlide();
}

function openSettings() {
  if (!settings) return;
  const currentSettings = settings;
  ++renderGeneration;
  workspace.classList.remove("preview-closed");
  selectedFile = null;
  previewPanel.hidden = false;
  previewToolbar.replaceChildren();
  element<HTMLHeadingElement>("previewTitle").textContent = "設定";
  element<HTMLSpanElement>("previewSubtitle").textContent = "このデバイスに保存されます";
  element<HTMLDivElement>("previewFileIcon").textContent = "⚙";
  const form = document.createElement("form");
  form.className = "settings-form";
  const themeOptions = COLOR_THEMES.map(
    (theme) =>
      `<option value="${theme.value}"${currentSettings.colorTheme === theme.value ? " selected" : ""}>${theme.label}</option>`,
  ).join("");
  form.innerHTML = `
    <label>カラーテーマ <select name="theme">${themeOptions}</select></label>
    <label>プレビューパネル幅 <input name="width" type="number" min="360" max="800" step="10" value="${settings.previewPanelWidth}"></label>
    <label>隠しファイルを表示 <input name="hidden" type="checkbox" ${settings.showHiddenFiles ? "checked" : ""}></label>
    <button class="button button-primary" type="submit">設定を保存</button>`;
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    void saveSettings(form);
  });
  previewContent.replaceChildren(form);
}

async function saveSettings(form: HTMLFormElement) {
  if (!settings) return;
  const widthInput = form.elements.namedItem("width");
  const hiddenInput = form.elements.namedItem("hidden");
  const themeInput = form.elements.namedItem("theme");
  if (!(widthInput instanceof HTMLInputElement) || !(hiddenInput instanceof HTMLInputElement)) return;
  const colorTheme =
    themeInput instanceof HTMLSelectElement &&
    COLOR_THEMES.some((theme) => theme.value === themeInput.value)
      ? (themeInput.value as ColorTheme)
      : "light";
  const nextSettings = {
    ...settings,
    previewPanelWidth: Number(widthInput.value),
    showHiddenFiles: hiddenInput.checked,
    colorTheme,
  } as Settings;
  try {
    await invoke("save_settings", { settings: nextSettings });
    settings = nextSettings;
    setPanelWidth(settings.previewPanelWidth);
    applyTheme(settings.colorTheme);
    notify("設定を保存しました。");
    if (currentRoot) await loadDirectory();
  } catch (error) {
    showError(error);
  }
}

function applyTheme(theme: ColorTheme) {
  document.documentElement.dataset.theme = theme;
}

function setPanelWidth(width: number) {
  document.documentElement.style.setProperty("--preview-width", `${width}px`);
}

function setSidebarWidth(width: number) {
  document.documentElement.style.setProperty("--sidebar-width", `${width}px`);
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

async function persistPanelWidths() {
  if (!settings) return;
  try {
    await invoke("save_settings", { settings });
  } catch (error) {
    showError(error);
  }
}

function setupResizeHandles() {
  const attach = (
    handle: HTMLDivElement,
    computeWidth: (clientX: number) => number,
    applyWidth: (width: number) => void,
  ) => {
    let dragging = false;
    const finish = () => {
      if (!dragging) return;
      dragging = false;
      document.body.classList.remove("resizing");
      void persistPanelWidths();
    };
    handle.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      dragging = true;
      document.body.classList.add("resizing");
      handle.setPointerCapture(event.pointerId);
    });
    handle.addEventListener("pointermove", (event) => {
      if (!dragging || !settings) return;
      applyWidth(computeWidth(event.clientX));
    });
    handle.addEventListener("pointerup", finish);
    handle.addEventListener("pointercancel", finish);
    handle.addEventListener("keydown", (event) => {
      if (!settings) return;
      const step = event.shiftKey ? 50 : 10;
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      const direction = event.key === "ArrowRight" ? 1 : -1;
      const rect = handle.getBoundingClientRect();
      applyWidth(computeWidth(rect.left + rect.width / 2 + direction * step));
      void persistPanelWidths();
    });
  };

  attach(
    sidebarResizeHandle,
    (clientX) => {
      const width = clamp(Math.round(clientX - workspace.getBoundingClientRect().left), 160, 400);
      if (settings) settings.sidebarWidth = width;
      return width;
    },
    setSidebarWidth,
  );
  attach(
    previewResizeHandle,
    (clientX) => {
      const width = clamp(Math.round(workspace.getBoundingClientRect().right - clientX), 360, 800);
      if (settings) settings.previewPanelWidth = width;
      return width;
    },
    setPanelWidth,
  );
}

function closePreview(reloadDirectory = true) {
  renderGeneration += 1;
  void currentPdfTask?.destroy();
  currentPdfTask = null;
  currentPdf = null;
  revokeObjectUrl();
  previewPanel.hidden = true;
  workspace.classList.add("preview-closed");
  previewContent.replaceChildren();
  previewToolbar.replaceChildren();
  selectedFile = null;
  if (reloadDirectory && currentRoot) void loadDirectory();
}

function revokeObjectUrl() {
  if (currentObjectUrl) URL.revokeObjectURL(currentObjectUrl);
  currentObjectUrl = null;
}

function toolbarButton(text: string, label: string) {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = text;
  button.setAttribute("aria-label", label);
  button.title = label;
  return button;
}

function message(text: string) {
  const node = document.createElement("div");
  node.className = "preview-empty";
  node.textContent = text;
  return node;
}

function renderError(error: unknown) {
  const card = document.createElement("div");
  card.className = "error-card";
  card.textContent = errorMessage(error);
  previewContent.replaceChildren(card);
}

function showError(error: unknown) {
  notify(errorMessage(error), true);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function notify(text: string, isError = false) {
  toast.textContent = text;
  toast.classList.toggle("error", isError);
  toast.classList.add("visible");
  window.clearTimeout(toastTimeout);
  toastTimeout = window.setTimeout(() => toast.classList.remove("visible"), 3500);
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[index]}`;
}

function formatDate(value: string | null) {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "—"
    : new Intl.DateTimeFormat("ja-JP", { dateStyle: "medium" }).format(date);
}

element<HTMLButtonElement>("openFolderButton").addEventListener("click", () => void chooseRoot());
element<HTMLButtonElement>("addRootButton").addEventListener("click", () => void chooseRoot());
element<HTMLButtonElement>("emptyOpenButton").addEventListener("click", () => void chooseRoot());
element<HTMLButtonElement>("settingsButton").addEventListener("click", openSettings);
element<HTMLButtonElement>("dismissAgentResultsButton").addEventListener("click", () => void dismissAllAgentResults());
element<HTMLButtonElement>("refreshButton").addEventListener("click", () => {
  cachedContentSearch = null;
  void loadDirectory();
});
element<HTMLButtonElement>("togglePreviewButton").addEventListener("click", () => closePreview());
element<HTMLButtonElement>("closePreviewButton").addEventListener("click", () => closePreview());
element<HTMLButtonElement>("previousPageButton").addEventListener("click", () => {
  if (page > 0) {
    page -= 1;
    void loadDirectory();
  }
});
element<HTMLButtonElement>("nextPageButton").addEventListener("click", () => {
  if ((page + 1) * PAGE_SIZE < total) {
    page += 1;
    void loadDirectory();
  }
});

let searchTimeout: number | undefined;
searchInput.addEventListener("input", () => {
  window.clearTimeout(searchTimeout);
  searchTimeout = window.setTimeout(() => {
    page = 0;
    void loadDirectory();
  }, 180);
});
searchMode.addEventListener("change", () => {
  page = 0;
  searchInput.placeholder =
    searchMode.value === "content" ? "ファイルの内容を検索" : "ファイルを検索";
  void loadDirectory();
});

window.addEventListener("keydown", (event) => {
  const target = event.target;
  const isTyping =
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable);
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
    event.preventDefault();
    searchInput.focus();
  } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "o") {
    event.preventDefault();
    if (event.shiftKey) {
      void openSelectedFileExternally();
    } else {
      void chooseRoot();
    }
  } else if (!isTyping && event.key === "Escape" && !previewPanel.hidden) {
    closePreview();
  } else if (!isTyping && event.key === "Backspace" && currentPath) {
    event.preventDefault();
    currentPath = currentPath.split("/").slice(0, -1).join("/");
    page = 0;
    void loadDirectory();
  } else if (!isTyping && event.key === "F5") {
    event.preventDefault();
    void loadDirectory();
  }
});

void initialize();
