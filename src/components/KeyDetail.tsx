import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Braces,
  Clock3,
  Copy,
  Database,
  KeyRound,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Trash2,
  X,
} from "lucide-react";
import type { KeyValue, ValueRow } from "../lib/types";
import { useAppStore } from "../stores/appStore";
import { Portal } from "./Portal";

type DetailTab = "value" | "info";
type EditDialog = {
  name: string;
  ttlMode: "never" | "custom";
  ttlValue: string;
} | null;

const typeClass: Record<string, string> = {
  string: "type-string",
  hash: "type-hash",
  list: "type-list",
  set: "type-set",
  zset: "type-zset",
  stream: "type-stream",
};

function typeLabel(type: string) {
  return type === "none" ? "已过期" : type;
}

function formatTtl(ttl: number) {
  if (ttl === -1) return "永久";
  if (ttl <= 0) return "已过期";
  if (ttl < 60) return `${ttl} 秒`;
  if (ttl < 3600) return `${Math.floor(ttl / 60)} 分 ${ttl % 60} 秒`;
  if (ttl < 86400) {
    const hours = Math.floor(ttl / 3600);
    const minutes = Math.floor((ttl % 3600) / 60);
    return `${hours} 小时 ${minutes} 分`;
  }
  return `${Math.floor(ttl / 86400)} 天 ${Math.floor((ttl % 86400) / 3600)} 小时`;
}

function formatBytes(bytes?: number | null) {
  if (bytes == null) return "-";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function ModalShell({
  title,
  children,
  onClose,
}: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
}) {
  return (
    <Portal>
      <div className="modal-backdrop" onMouseDown={onClose}>
        <div
          className="modal modal-small"
          onMouseDown={(event) => event.stopPropagation()}
        >
          <div className="modal-head">
            <h3>{title}</h3>
            <button className="icon-btn" onClick={onClose} title="关闭">
              <X size={16} />
            </button>
          </div>
          {children}
        </div>
      </div>
    </Portal>
  );
}

export function KeyDetail() {
  const connection = useAppStore((state) => state.connection);
  const selectedKey = useAppStore((state) => state.selectedKey);
  const keyInfo = useAppStore((state) => state.keyInfo);
  const keyValue = useAppStore((state) => state.keyValue);
  const detailLoading = useAppStore((state) => state.detailLoading);
  const readOnly = useAppStore((state) => state.readOnly);
  const reloadSelected = useAppStore((state) => state.reloadSelected);
  const loadMoreValue = useAppStore((state) => state.loadMoreValue);
  const deleteSelected = useAppStore((state) => state.deleteSelected);
  const updateSelectedKey = useAppStore((state) => state.updateSelectedKey);
  const saveString = useAppStore((state) => state.saveString);
  const hashSet = useAppStore((state) => state.hashSet);
  const hashDelete = useAppStore((state) => state.hashDelete);
  const listPush = useAppStore((state) => state.listPush);
  const listSet = useAppStore((state) => state.listSet);
  const listRemove = useAppStore((state) => state.listRemove);
  const setAdd = useAppStore((state) => state.setAdd);
  const setRemove = useAppStore((state) => state.setRemove);
  const zsetAdd = useAppStore((state) => state.zsetAdd);
  const zsetRemove = useAppStore((state) => state.zsetRemove);
  const showNotice = useAppStore((state) => state.showNotice);

  const [tab, setTab] = useState<DetailTab>("value");
  const [edit, setEdit] = useState<EditDialog>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [liveTtl, setLiveTtl] = useState(-2);

  useEffect(() => {
    setTab("value");
    setEdit(null);
    setConfirmDelete(false);
  }, [selectedKey]);

  useEffect(() => {
    const ttl = keyInfo?.ttl;
    if (ttl == null) {
      setLiveTtl(-2);
      return;
    }
    setLiveTtl(ttl);
    if (ttl <= 0) return;

    const startedAt = Date.now();
    const timer = window.setInterval(() => {
      const remaining = ttl - Math.floor((Date.now() - startedAt) / 1000);
      setLiveTtl(Math.max(remaining, 0));
      if (remaining <= 0) window.clearInterval(timer);
    }, 1000);

    return () => window.clearInterval(timer);
  }, [keyInfo, selectedKey]);

  if (!connection) {
    return (
      <section className="detail-panel">
        <div className="welcome">
          <div className="welcome-mark">LR</div>
          <h2>本地 Redis，一目了然</h2>
          <p>连接任意可访问的 Redis 服务，支持本机、局域网与公网。</p>
          <div className="welcome-steps">
            <div>
              <span>01</span>
              新建连接
            </div>
            <div>
              <span>02</span>
              选择键
            </div>
            <div>
              <span>03</span>
              查看或编辑
            </div>
          </div>
          <div className="welcome-shortcuts">
            <kbd>Ctrl</kbd> + <kbd>K</kbd> 搜索键
            <span className="welcome-divider" />
            <kbd>F5</kbd> 刷新
          </div>
        </div>
      </section>
    );
  }

  if (!selectedKey) {
    return (
      <section className="detail-panel">
        <div className="panel-empty detail-empty">
          <KeyRound size={30} />
          <p>选择一个键查看详情</p>
          <span>左侧列表使用 SCAN 游标加载，不会阻塞 Redis</span>
        </div>
      </section>
    );
  }

  const type = keyInfo?.keyType ?? keyValue?.kind ?? "unknown";

  const copyKey = async () => {
    try {
      await navigator.clipboard.writeText(selectedKey);
      showNotice("success", "键名已复制");
    } catch {
      showNotice("error", "当前环境无法访问剪贴板");
    }
  };

  const openEdit = () => {
    const ttl = liveTtl > 0 ? liveTtl : -1;
    setEdit({
      name: selectedKey,
      ttlMode: ttl > 0 ? "custom" : "never",
      ttlValue: ttl > 0 ? String(ttl) : "3600",
    });
  };

  const submitEdit = async () => {
    if (!edit) return;
    const name = edit.name.trim();
    if (!name) {
      showNotice("error", "键名不能为空");
      return;
    }
    let ttlSeconds: number | null = null;
    if (edit.ttlMode === "custom") {
      const seconds = Number(edit.ttlValue);
      if (!Number.isFinite(seconds) || seconds <= 0) {
        showNotice("error", "过期时间必须是大于 0 的秒数");
        return;
      }
      ttlSeconds = Math.floor(seconds);
    }
    const updated = await updateSelectedKey(name, ttlSeconds);
    if (updated) setEdit(null);
  };

  return (
    <section
      className={`detail-panel ${detailLoading ? "is-loading" : ""}`}
      aria-busy={detailLoading}
    >
      <div className="detail-head">
        <div className="detail-key">
          <div className="detail-key-line">
            <span className={`type-badge ${typeClass[type] ?? ""}`}>
              {typeLabel(type)}
            </span>
            <span className="detail-key-name" title={selectedKey}>
              {selectedKey}
            </span>
          </div>
          <div className="detail-meta">
            <span>
              <Clock3 size={12} /> TTL {formatTtl(liveTtl)}
            </span>
            <span>
              <Database size={12} /> {formatBytes(keyInfo?.size)}
            </span>
            {keyInfo?.encoding && <span>编码 {keyInfo.encoding}</span>}
          </div>
        </div>
        <div className="detail-actions">
          <button className="icon-btn" title="复制键名" onClick={() => void copyKey()}>
            <Copy size={15} />
          </button>
          <button
            className="icon-btn"
            title="刷新"
            onClick={() => void reloadSelected()}
            disabled={detailLoading}
          >
            <RefreshCw size={15} className={detailLoading ? "spin" : ""} />
          </button>
          <button
            className="icon-btn"
            title="编辑键名和过期时间"
            disabled={readOnly || type === "none"}
            onClick={openEdit}
          >
            <Pencil size={15} />
          </button>
          <button
            className="icon-btn danger"
            title="删除键"
            disabled={readOnly || type === "none"}
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 size={15} />
          </button>
        </div>
      </div>

      <div className="detail-tabs">
        <button
          className={tab === "value" ? "active" : ""}
          onClick={() => setTab("value")}
        >
          值
        </button>
        <button
          className={tab === "info" ? "active" : ""}
          onClick={() => setTab("info")}
        >
          信息
        </button>
        {readOnly && <span className="readonly-badge">只读模式</span>}
      </div>

      <div className="detail-body">
        {detailLoading && keyInfo && (
          <div className="detail-loading-overlay">
            <div className="detail-loading-pill">
              <span className="mini-spinner" />
              正在刷新
            </div>
          </div>
        )}
        {detailLoading && !keyInfo && (
          <div className="detail-skeleton">
            <div />
            <div />
            <div />
          </div>
        )}

        {!detailLoading && type === "none" && (
          <div className="inline-warning">
            <AlertTriangle size={16} />
            这个键已经不存在，可能刚被删除或已经过期。
          </div>
        )}

        {keyInfo && keyValue && tab === "value" && (
          <>
            {keyValue.message && (
              <div className="inline-warning">
                <AlertTriangle size={16} />
                {keyValue.message}
              </div>
            )}
            <ValueEditor
              key={selectedKey}
              keyValue={keyValue}
              readOnly={readOnly}
              onSaveString={(value) => void saveString(value)}
              onHashSet={(field, value) => void hashSet(field, value)}
              onHashDelete={(field) => void hashDelete(field)}
              onListPush={(value, side) => void listPush(value, side)}
              onListSet={(index, value) => void listSet(index, value)}
              onListRemove={(value) => void listRemove(value)}
              onSetAdd={(member) => void setAdd(member)}
              onSetRemove={(member) => void setRemove(member)}
              onZsetAdd={(member, score) => void zsetAdd(member, score)}
              onZsetRemove={(member) => void zsetRemove(member)}
              onLoadMore={() => void loadMoreValue()}
            />
          </>
        )}

        {keyInfo && tab === "info" && (
          <div className="info-panel">
            <div className="info-grid">
              <InfoCard label="键名" value={selectedKey} mono />
              <InfoCard label="类型" value={typeLabel(type)} />
              <InfoCard label="TTL" value={formatTtl(liveTtl)} />
              <InfoCard label="内存占用" value={formatBytes(keyInfo.size)} />
              <InfoCard label="内部编码" value={keyInfo.encoding ?? "-"} />
              <InfoCard
                label={keyValue?.kind === "string" ? "字符串大小" : "已加载"}
                value={
                  keyValue?.kind === "string"
                    ? `${keyValue.total} bytes`
                    : `${keyValue?.rows.length ?? 0} / ${keyValue?.total ?? 0}`
                }
              />
            </div>
            <div className="info-note">
              值列表按需分页读取。大 Key 不会一次性加载到界面，避免阻塞 Redis
              和 WebView。
            </div>
          </div>
        )}
      </div>

      {edit && (
        <ModalShell title="编辑键" onClose={() => setEdit(null)}>
          <div className="modal-body">
            <div className="field">
              <label className="field-label">键名</label>
              <input
                className="text-input"
                autoFocus
                value={edit.name}
                onChange={(event) =>
                  setEdit({ ...edit, name: event.target.value })
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter") void submitEdit();
                }}
              />
            </div>

            <div className="field">
              <label className="field-label">过期时间</label>
              <div className="ttl-options">
                <button
                  type="button"
                  className={`ttl-option ${
                    edit.ttlMode === "never" ? "active" : ""
                  }`}
                  onClick={() =>
                    setEdit({ ...edit, ttlMode: "never" })
                  }
                >
                  永不过期
                </button>
                <button
                  type="button"
                  className={`ttl-option ${
                    edit.ttlMode === "custom" ? "active" : ""
                  }`}
                  onClick={() =>
                    setEdit({ ...edit, ttlMode: "custom" })
                  }
                >
                  自定义
                </button>
              </div>
            </div>

            {edit.ttlMode === "custom" && (
              <div className="field">
                <label className="field-label">过期秒数</label>
                <div className="ttl-input-row">
                  <input
                    className="text-input"
                    type="number"
                    min={1}
                    value={edit.ttlValue}
                    onChange={(event) =>
                      setEdit({ ...edit, ttlValue: event.target.value })
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void submitEdit();
                    }}
                  />
                  <span className="ttl-unit">秒</span>
                </div>
                <div className="ttl-presets">
                  {[
                    ["1 小时", "3600"],
                    ["1 天", "86400"],
                    ["7 天", "604800"],
                    ["30 天", "2592000"],
                  ].map(([label, seconds]) => (
                    <button
                      key={seconds}
                      type="button"
                      onClick={() =>
                        setEdit({ ...edit, ttlValue: seconds })
                      }
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>
            )}

            <p className="field-help">
              当前 TTL：{formatTtl(liveTtl)}。如果新键名已存在，会被直接覆盖。
            </p>
          </div>
          <div className="modal-footer">
            <button className="btn ghost" onClick={() => setEdit(null)}>
              取消
            </button>
            <button
              className="btn primary"
              disabled={!edit.name.trim()}
              onClick={() => void submitEdit()}
            >
              保存修改
            </button>
          </div>
        </ModalShell>
      )}

      {confirmDelete && (
        <ModalShell title="删除键" onClose={() => setConfirmDelete(false)}>
          <div className="modal-body">
            <div className="danger-box">
              <AlertTriangle size={18} />
              <div>
                <p>确定要删除这个键吗？</p>
                <code>{selectedKey}</code>
                <span>此操作不可撤销。</span>
              </div>
            </div>
          </div>
          <div className="modal-footer">
            <button
              className="btn ghost"
              onClick={() => setConfirmDelete(false)}
            >
              取消
            </button>
            <button
              className="btn danger"
              onClick={() => {
                setConfirmDelete(false);
                void deleteSelected();
              }}
            >
              删除
            </button>
          </div>
        </ModalShell>
      )}
    </section>
  );
}

function InfoCard({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="info-card">
      <div className="info-label">{label}</div>
      <div className={`info-value ${mono ? "mono" : ""}`} title={value}>
        {value}
      </div>
    </div>
  );
}

interface ValueEditorProps {
  keyValue: KeyValue;
  readOnly: boolean;
  onSaveString: (value: string) => void;
  onHashSet: (field: string, value: string) => void;
  onHashDelete: (field: string) => void;
  onListPush: (value: string, side: "left" | "right") => void;
  onListSet: (index: number, value: string) => void;
  onListRemove: (value: string) => void;
  onSetAdd: (member: string) => void;
  onSetRemove: (member: string) => void;
  onZsetAdd: (member: string, score: number) => void;
  onZsetRemove: (member: string) => void;
  onLoadMore: () => void;
}

function ValueEditor(props: ValueEditorProps) {
  const { keyValue, onLoadMore, readOnly } = props;
  const canLoadMore = keyValue.cursor !== 0 && keyValue.rows.length > 0;
  const moreButton = canLoadMore ? (
    <button className="load-more value-load-more" onClick={onLoadMore}>
      加载更多（已显示 {keyValue.rows.length} / {keyValue.total}）
    </button>
  ) : null;

  if (keyValue.kind === "string") {
    return (
      <StringEditor
        value={keyValue.value}
        truncated={keyValue.truncated}
        readOnly={readOnly}
        onSave={props.onSaveString}
      />
    );
  }

  if (keyValue.kind === "hash") {
    return (
      <>
        <HashEditor
          rows={keyValue.rows}
          readOnly={readOnly}
          onSet={props.onHashSet}
          onDelete={props.onHashDelete}
        />
        {moreButton}
      </>
    );
  }

  if (keyValue.kind === "list") {
    return (
      <>
        <ListEditor
          rows={keyValue.rows}
          readOnly={readOnly}
          onPush={props.onListPush}
          onSet={props.onListSet}
          onRemove={props.onListRemove}
        />
        {moreButton}
      </>
    );
  }

  if (keyValue.kind === "set") {
    return (
      <>
        <SetEditor
          rows={keyValue.rows}
          readOnly={readOnly}
          onAdd={props.onSetAdd}
          onRemove={props.onSetRemove}
        />
        {moreButton}
      </>
    );
  }

  if (keyValue.kind === "zset") {
    return (
      <>
        <ZSetEditor
          rows={keyValue.rows}
          readOnly={readOnly}
          onAdd={props.onZsetAdd}
          onRemove={props.onZsetRemove}
        />
        {moreButton}
      </>
    );
  }

  return (
    <div className="panel-empty detail-empty">
      <Database size={24} />
      <p>暂不支持可视化编辑 {keyValue.kind}</p>
      <span>可以在后续版本中补充可视化操作</span>
    </div>
  );
}

function StringEditor({
  value,
  truncated,
  readOnly,
  onSave,
}: {
  value: string;
  truncated: boolean;
  readOnly: boolean;
  onSave: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const [formatError, setFormatError] = useState<string | null>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const gutterRef = useRef<HTMLDivElement>(null);

  const lineCount = draft.split("\n").length;
  const gutterWidth = `${Math.max(2, String(lineCount).length) + 1.5}ch`;

  useEffect(() => {
    setDraft(value);
    setFormatError(null);
  }, [value]);

  const syncScroll = () => {
    if (editorRef.current && gutterRef.current) {
      gutterRef.current.scrollTop = editorRef.current.scrollTop;
    }
  };

  useEffect(() => {
    syncScroll();
  }, [draft]);

  const dirty = draft !== value;
  const bytes = new TextEncoder().encode(draft).length;

  const formatJson = () => {
    try {
      const parsed = JSON.parse(draft);
      setDraft(JSON.stringify(parsed, null, 2));
      setFormatError(null);
    } catch {
      setFormatError("不是合法的 JSON，无法格式化");
    }
  };

  return (
    <div className="string-editor-wrap">
      <div className="editor-toolbar">
        <span className="editor-label">
          String
          <span className="editor-size">{bytes} bytes</span>
        </span>
        <div className="editor-toolbar-actions">
          <button
            className="btn ghost small"
            onClick={formatJson}
            disabled={readOnly || truncated}
          >
            <Braces size={13} /> 格式化 JSON
          </button>
          <button
            className="btn primary small"
            disabled={!dirty || readOnly || truncated}
            onClick={() => onSave(draft)}
          >
            <Save size={13} /> 保存
          </button>
        </div>
      </div>
      {truncated && (
        <div className="inline-warning">
          <AlertTriangle size={15} />
          值超过 64 KB，当前仅展示前 64 KB，已禁用直接编辑以避免覆盖原始数据。
        </div>
      )}
      <div className="string-editor-shell">
        <div
          ref={gutterRef}
          className="line-numbers"
          aria-hidden="true"
          style={{ width: gutterWidth }}
        >
          {Array.from({ length: lineCount }, (_, index) => (
            <span className="line-number" key={index}>
              {index + 1}
            </span>
          ))}
        </div>
        <textarea
          ref={editorRef}
          className="string-editor"
          value={draft}
          wrap="off"
          spellCheck={false}
          readOnly={readOnly || truncated}
          onScroll={syncScroll}
          onChange={(event) => {
            setDraft(event.target.value);
            setFormatError(null);
          }}
        />
      </div>
      <div className="editor-footer">
        <span>{formatError ?? (dirty ? "有未保存的修改" : "内容未修改")}</span>
        <span>{readOnly ? "只读模式已开启" : "修改后点击保存"}</span>
      </div>
    </div>
  );
}

function HashEditor({
  rows,
  readOnly,
  onSet,
  onDelete,
}: {
  rows: ValueRow[];
  readOnly: boolean;
  onSet: (field: string, value: string) => void;
  onDelete: (field: string) => void;
}) {
  const [field, setField] = useState("");
  const [value, setValue] = useState("");
  const [editingField, setEditingField] = useState<string | null>(null);
  const [editingValue, setEditingValue] = useState("");

  const add = () => {
    if (!field.trim()) return;
    onSet(field, value);
    setField("");
    setValue("");
  };

  return (
    <>
      <div className="row-add">
        <input
          className="text-input"
          placeholder="字段名"
          value={field}
          onChange={(event) => setField(event.target.value)}
          disabled={readOnly}
        />
        <input
          className="text-input"
          placeholder="值"
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") add();
          }}
          disabled={readOnly}
        />
        <button
          className="btn primary small"
          onClick={add}
          disabled={readOnly || !field.trim()}
        >
          <Plus size={13} /> 添加 / 更新
        </button>
      </div>

      <div className="value-table hash-table">
        <div className="value-table-head">
          <span>字段</span>
          <span>值</span>
          <span />
        </div>
        {rows.length === 0 && <div className="table-empty">Hash 为空</div>}
        {rows.map((row) => (
          <div className="value-row" key={row.field}>
            <span className="cell-field mono" title={row.field}>
              {row.field}
            </span>
            {editingField === row.field ? (
              <input
                className="text-input inline-input"
                autoFocus
                value={editingValue}
                onChange={(event) => setEditingValue(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    onSet(row.field, editingValue);
                    setEditingField(null);
                  }
                  if (event.key === "Escape") setEditingField(null);
                }}
              />
            ) : (
              <span className="cell-value mono" title={row.value}>
                {row.value}
              </span>
            )}
            <span className="cell-actions">
              {editingField === row.field ? (
                <>
                  <button
                    className="icon-btn tiny"
                    title="保存"
                    onClick={() => {
                      onSet(row.field, editingValue);
                      setEditingField(null);
                    }}
                  >
                    <Save size={12} />
                  </button>
                  <button
                    className="icon-btn tiny"
                    title="取消"
                    onClick={() => setEditingField(null)}
                  >
                    <X size={12} />
                  </button>
                </>
              ) : (
                <>
                  <button
                    className="icon-btn tiny"
                    title="编辑"
                    disabled={readOnly}
                    onClick={() => {
                      setEditingField(row.field);
                      setEditingValue(row.value);
                    }}
                  >
                    <Pencil size={12} />
                  </button>
                  <button
                    className="icon-btn tiny danger"
                    title="删除字段"
                    disabled={readOnly}
                    onClick={() => onDelete(row.field)}
                  >
                    <Trash2 size={12} />
                  </button>
                </>
              )}
            </span>
          </div>
        ))}
      </div>
    </>
  );
}

function SetEditor({
  rows,
  readOnly,
  onAdd,
  onRemove,
}: {
  rows: ValueRow[];
  readOnly: boolean;
  onAdd: (member: string) => void;
  onRemove: (member: string) => void;
}) {
  const [member, setMember] = useState("");

  const add = () => {
    if (!member.trim()) return;
    onAdd(member);
    setMember("");
  };

  return (
    <>
      <div className="row-add">
        <input
          className="text-input"
          placeholder="新成员"
          value={member}
          onChange={(event) => setMember(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") add();
          }}
          disabled={readOnly}
        />
        <button
          className="btn primary small"
          onClick={add}
          disabled={readOnly || !member.trim()}
        >
          <Plus size={13} /> 添加成员
        </button>
      </div>
      <div className="value-table set-table">
        {rows.length === 0 && <div className="table-empty">Set 为空</div>}
        {rows.map((row) => (
          <div className="value-row set-row" key={row.field}>
            <span className="cell-field mono" title={row.field}>
              {row.field}
            </span>
            <span />
            <span className="cell-actions">
              <button
                className="icon-btn tiny danger"
                title="删除成员"
                disabled={readOnly}
                onClick={() => onRemove(row.field)}
              >
                <Trash2 size={12} />
              </button>
            </span>
          </div>
        ))}
      </div>
    </>
  );
}

function ZSetEditor({
  rows,
  readOnly,
  onAdd,
  onRemove,
}: {
  rows: ValueRow[];
  readOnly: boolean;
  onAdd: (member: string, score: number) => void;
  onRemove: (member: string) => void;
}) {
  const [member, setMember] = useState("");
  const [score, setScore] = useState("0");

  const add = () => {
    const parsed = Number(score);
    if (!member.trim() || !Number.isFinite(parsed)) return;
    onAdd(member, parsed);
    setMember("");
    setScore("0");
  };

  return (
    <>
      <div className="row-add">
        <input
          className="text-input"
          placeholder="成员"
          value={member}
          onChange={(event) => setMember(event.target.value)}
          disabled={readOnly}
        />
        <input
          className="text-input score-input"
          placeholder="分数"
          type="number"
          value={score}
          onChange={(event) => setScore(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") add();
          }}
          disabled={readOnly}
        />
        <button
          className="btn primary small"
          onClick={add}
          disabled={readOnly || !member.trim() || !Number.isFinite(Number(score))}
        >
          <Plus size={13} /> 添加 / 更新
        </button>
      </div>
      <div className="value-table zset-table">
        <div className="value-table-head">
          <span>成员</span>
          <span>分数</span>
          <span />
        </div>
        {rows.length === 0 && <div className="table-empty">ZSet 为空</div>}
        {rows.map((row) => (
          <div className="value-row" key={row.field}>
            <span className="cell-field mono" title={row.field}>
              {row.field}
            </span>
            <span className="cell-value score-value mono">{row.score ?? "-"}</span>
            <span className="cell-actions">
              <button
                className="icon-btn tiny danger"
                title="删除成员"
                disabled={readOnly}
                onClick={() => onRemove(row.field)}
              >
                <Trash2 size={12} />
              </button>
            </span>
          </div>
        ))}
      </div>
    </>
  );
}

function ListEditor({
  rows,
  readOnly,
  onPush,
  onSet,
  onRemove,
}: {
  rows: ValueRow[];
  readOnly: boolean;
  onPush: (value: string, side: "left" | "right") => void;
  onSet: (index: number, value: string) => void;
  onRemove: (value: string) => void;
}) {
  const [value, setValue] = useState("");
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editingValue, setEditingValue] = useState("");

  return (
    <>
      <div className="row-add">
        <input
          className="text-input"
          placeholder="新元素"
          value={value}
          onChange={(event) => setValue(event.target.value)}
          disabled={readOnly}
        />
        <button
          className="btn primary small"
          title="追加到列表末尾"
          disabled={readOnly || !value}
          onClick={() => {
            onPush(value, "right");
            setValue("");
          }}
        >
          <Plus size={13} /> 插入
        </button>
      </div>
      <div className="value-table list-table">
        <div className="value-table-head">
          <span>索引</span>
          <span>值</span>
          <span />
        </div>
        {rows.length === 0 && (
          <div className="table-empty">List 为空，使用上方「插入」添加第一个元素</div>
        )}
        {rows.map((row) => {
          const index = Number(row.field);
          return (
            <div className="value-row list-row" key={row.field}>
              <span className="cell-index mono">{row.field}</span>
              {editingIndex === index ? (
                <input
                  className="text-input inline-input"
                  autoFocus
                  value={editingValue}
                  onChange={(event) => setEditingValue(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      onSet(index, editingValue);
                      setEditingIndex(null);
                    }
                    if (event.key === "Escape") setEditingIndex(null);
                  }}
                />
              ) : (
                <span className="cell-value mono" title={row.value}>
                  {row.value}
                </span>
              )}
              <span className="cell-actions">
                {editingIndex === index ? (
                  <>
                    <button
                      className="icon-btn tiny"
                      title="保存"
                      onClick={() => {
                        onSet(index, editingValue);
                        setEditingIndex(null);
                      }}
                    >
                      <Save size={12} />
                    </button>
                    <button
                      className="icon-btn tiny"
                      title="取消"
                      onClick={() => setEditingIndex(null)}
                    >
                      <X size={12} />
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      className="icon-btn tiny"
                      title="编辑"
                      disabled={readOnly}
                      onClick={() => {
                        setEditingIndex(index);
                        setEditingValue(row.value);
                      }}
                    >
                      <Pencil size={12} />
                    </button>
                    <button
                      className="icon-btn tiny danger"
                      title="删除元素"
                      disabled={readOnly}
                      onClick={() => onRemove(row.value)}
                    >
                      <Trash2 size={12} />
                    </button>
                  </>
                )}
              </span>
            </div>
          );
        })}
      </div>
    </>
  );
}
