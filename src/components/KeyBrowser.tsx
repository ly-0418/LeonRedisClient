import { useLayoutEffect, useRef, useState } from "react";
import {
  ChevronDown,
  KeyRound,
  Loader2,
  Plus,
  RefreshCw,
  Search,
  X,
} from "lucide-react";
import type { KeyItem } from "../lib/types";
import { useAppStore } from "../stores/appStore";
import { DbSelector } from "./DbSelector";
import { NewKeyDialog } from "./NewKeyDialog";

const typeOptions = [
  { value: "", label: "全部" },
  { value: "string", label: "String" },
  { value: "hash", label: "Hash" },
  { value: "list", label: "List" },
  { value: "set", label: "Set" },
  { value: "zset", label: "ZSet" },
  { value: "stream", label: "Stream" },
];

const typeClass: Record<string, string> = {
  string: "type-string",
  hash: "type-hash",
  list: "type-list",
  set: "type-set",
  zset: "type-zset",
  stream: "type-stream",
};

const typeLabel: Record<string, string> = {
  string: "String",
  hash: "Hash",
  list: "List",
  set: "Set",
  zset: "ZSet",
  stream: "Stream",
};

/// 首次渲染时的行高估算，渲染后按真实行高测量并校正。
const KEY_ROW_ESTIMATE = 32;
const OVERSCAN = 8;

/**
 * 窗口化的键列表：只挂载可见区域的若干行。
 *
 * 十几万个键时仍然只渲染几十个 DOM 节点，「继续加载」也不会让界面越来越卡。
 */
function VirtualKeyList({
  keys,
  selectedKey,
  refreshing,
  onSelect,
  children,
}: {
  keys: KeyItem[];
  selectedKey: string | null;
  refreshing: boolean;
  onSelect: (name: string) => void;
  children: React.ReactNode;
}) {
  const listRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [rowHeight, setRowHeight] = useState(KEY_ROW_ESTIMATE);

  useLayoutEffect(() => {
    const element = listRef.current;
    if (!element) return;
    const measure = () => setViewportHeight(element.clientHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useLayoutEffect(() => {
    const row = listRef.current?.querySelector<HTMLElement>(".key-row");
    if (!row) return;
    const style = window.getComputedStyle(row);
    const measured =
      row.offsetHeight +
      (parseFloat(style.marginTop) || 0) +
      (parseFloat(style.marginBottom) || 0);
    if (measured > 0 && Math.abs(measured - rowHeight) > 0.5) {
      setRowHeight(measured);
    }
  }, [keys.length, rowHeight]);

  const totalHeight = keys.length * rowHeight;
  const visibleCount = Math.ceil(viewportHeight / rowHeight) + OVERSCAN * 2;
  const start =
    keys.length === 0
      ? 0
      : Math.max(
          0,
          Math.min(keys.length - 1, Math.floor(scrollTop / rowHeight) - OVERSCAN),
        );
  const end = Math.min(keys.length, start + visibleCount);
  const slice = keys.slice(start, end);

  return (
    <div
      ref={listRef}
      className={`key-list ${refreshing ? "is-refreshing" : ""}`}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      {children}
      {keys.length > 0 && (
        <div style={{ height: totalHeight, position: "relative" }}>
          <div style={{ transform: `translateY(${start * rowHeight}px)` }}>
            {slice.map((key) => (
              <button
                key={key.name}
                className={`key-row ${
                  selectedKey === key.name ? "selected" : ""
                }`}
                onClick={() => onSelect(key.name)}
                title={key.name}
              >
                <KeyRound size={13} />
                <span className="key-name">{key.name}</span>
                {key.keyType && key.keyType !== "unknown" && (
                  <span
                    className={`type-badge key-type ${
                      typeClass[key.keyType] ?? ""
                    }`}
                  >
                    {typeLabel[key.keyType] ?? key.keyType}
                  </span>
                )}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export function KeyBrowser() {
  const connection = useAppStore((state) => state.connection);
  const keys = useAppStore((state) => state.keys);
  const selectedKey = useAppStore((state) => state.selectedKey);
  const scanning = useAppStore((state) => state.scanning);
  const refreshingKeys = useAppStore((state) => state.refreshingKeys);
  const scanDone = useAppStore((state) => state.scanDone);
  const pattern = useAppStore((state) => state.pattern);
  const typeFilter = useAppStore((state) => state.typeFilter);
  const setPattern = useAppStore((state) => state.setPattern);
  const setTypeFilter = useAppStore((state) => state.setTypeFilter);
  const refreshKeys = useAppStore((state) => state.refreshKeys);
  const loadMoreKeys = useAppStore((state) => state.loadMoreKeys);
  const selectKey = useAppStore((state) => state.selectKey);
  const [newKeyOpen, setNewKeyOpen] = useState(false);

  return (
    <section className="key-panel" aria-busy={refreshingKeys}>
      <div
        className={`key-refresh-bar ${refreshingKeys ? "active" : ""}`}
        aria-hidden="true"
      />
      <DbSelector />

      <div className="panel-head">
        <div>
          <div className="panel-title">键浏览器</div>
          <div className="panel-sub">
            {connection
              ? typeFilter
                ? `类型：${typeFilter}`
                : "SCAN 游标扫描"
              : "尚未连接"}
          </div>
        </div>
        <div className="panel-head-actions">
          <button
            className="panel-add-btn"
            title="新建键"
            onClick={() => setNewKeyOpen(true)}
            disabled={!connection}
          >
            <Plus size={13} /> 新建
          </button>
          <button
            className="icon-btn"
            title="刷新键列表"
            onClick={() => void refreshKeys()}
            disabled={!connection || scanning}
          >
            <RefreshCw size={15} className={scanning ? "spin" : ""} />
          </button>
        </div>
      </div>

      <div className="key-search">
        <Search size={14} />
        <input
          id="key-search"
          value={pattern}
          spellCheck={false}
          placeholder="匹配模式，例如 user:*"
          onChange={(event) => setPattern(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void refreshKeys();
          }}
          disabled={!connection}
        />
        {pattern && pattern !== "*" && (
          <button
            className="icon-btn tiny"
            title="清除模式"
            onClick={() => {
              setPattern("*");
              void refreshKeys();
            }}
          >
            <X size={13} />
          </button>
        )}
      </div>

      <div className="type-filter">
        {typeOptions.map((option) => (
          <button
            key={option.value}
            className={`filter-chip ${typeFilter === option.value ? "active" : ""}`}
            onClick={() => setTypeFilter(option.value)}
            disabled={!connection}
          >
            {option.label}
          </button>
        ))}
      </div>

      <VirtualKeyList
        keys={keys}
        selectedKey={selectedKey}
        refreshing={refreshingKeys}
        onSelect={(name) => void selectKey(name)}
      >
        {!connection && (
          <div className="panel-empty">
            <KeyRound size={26} />
            <p>连接 Redis 后开始浏览键</p>
            <span>支持 String / Hash / List / Set / ZSet</span>
          </div>
        )}

        {connection && scanning && keys.length === 0 && (
          <div className="key-skeletons">
            {Array.from({ length: 8 }).map((_, index) => (
              <div className="key-skeleton" key={index} />
            ))}
          </div>
        )}
        {connection && !scanning && keys.length === 0 && (
          <div className="panel-empty">
            <Search size={24} />
            <p>没有匹配的键</p>
            <span>尝试修改匹配模式或类型筛选</span>
          </div>
        )}
      </VirtualKeyList>

      {connection && !scanDone && (
        <div className="key-panel-footer">
          <button
            className="load-more"
            onClick={() => void loadMoreKeys()}
            disabled={scanning}
          >
            {scanning ? (
              <>
                <Loader2 size={13} className="spin" /> 扫描中
              </>
            ) : (
              <>
                继续加载 <ChevronDown size={13} />
              </>
            )}
          </button>
        </div>
      )}

      {newKeyOpen && <NewKeyDialog onClose={() => setNewKeyOpen(false)} />}
    </section>
  );
}
