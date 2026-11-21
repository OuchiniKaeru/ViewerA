export type ContentSearchPage<T> = {
  items: T[];
  page: number;
  pageSize: number;
  total: number;
};

export function pageContentSearchHits<T>(
  hits: readonly T[],
  page: number,
  pageSize: number,
): ContentSearchPage<T> {
  const normalizedPageSize = Number.isFinite(pageSize)
    ? Math.max(1, Math.trunc(pageSize))
    : 1;
  const requestedPage = Number.isFinite(page) ? Math.max(0, Math.trunc(page)) : 0;
  const lastPage = Math.max(Math.ceil(hits.length / normalizedPageSize) - 1, 0);
  const currentPage = Math.min(requestedPage, lastPage);
  const start = currentPage * normalizedPageSize;
  return {
    items: hits.slice(start, start + normalizedPageSize),
    page: currentPage,
    pageSize: normalizedPageSize,
    total: hits.length,
  };
}
