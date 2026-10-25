import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Database, Loader2, Trash2 } from "lucide-react";
import { useAppStore } from "../stores/appStore";
import { ClearDatabaseDialog } from "./ClearDatabaseDialog";

export function DbSelector() {
  const connection = useAppStore((state) => state.connection);
  const databases = useAppStore((state) => state.databases);
  const maxDatabases = useAppStore((state) => state.maxDatabases);
  const loading = useAppStore((state) => state.databaseLoading);
  const readOnly = useAppStore((state) => state.readOnly);
  const selectDatabase = useAppStore((state) => state.selectDatabase);

  const [open, setOpen] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const pickerRef = useRef<HTMLDivElement>(null);

  const current = databases.find((db) => db.index === connection?.db);
  const currentKeys = current?.keys ?? connection?.dbSize ?? 0;
  const currentDb = connection?.db ?? 0;
  const options =
    databases.length > 0
      ? databases
      : [{ index: currentDb, keys: currentKeys }];

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!pickerRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const choose = (db: number) => {
    setOpen(false);
    void selectDatabase(db);
  };

  return (
    <div className="db-strip">
      <div className="db-picker" ref={pickerRef}>
        <button
          type="button"
          className={`db-trigger ${open ? "open" : ""}`}
          onClick={() => setOpen((value) => !value)}
          disabled={!connection || loading}
          aria-haspopup="listbox"
          aria-expanded={open}
          title={
            maxDatabases > 0
              ? `共 ${maxDatabases} 个数据库，当前 ${currentKeys} 个键`
              : "选择数据库"
          }
        >
          <Database size={14} />
          <span className="db-trigger-label">数据库</span>
          <span className="db-trigger-name">db{currentDb}</span>
          <span className="db-trigger-count">
            {loading ? <Loader2 size={12} className="spin" /> : `${currentKeys} 键`}
          </span>
          <ChevronDown size={14} className="db-trigger-chevron" />
        </button>

        {open && (
          <div className="db-menu" role="listbox" aria-label="选择数据库">
            <div className="db-menu-head">
              <span>切换数据库</span>
              <span>{maxDatabases > 0 ? `共 ${maxDatabases} 个` : ""}</span>
            </div>
            <div className="db-menu-list">
              {options.map((db) => {
                const active = db.index === currentDb;
                return (
                  <button
                    key={db.index}
                    type="button"
                    role="option"
                    aria-selected={active}
                    className={`db-option ${active ? "active" : ""}`}
                    onClick={() => choose(db.index)}
                  >
                    <span className="db-option-name">db{db.index}</span>
                    <span className="db-option-count">{db.keys} 键</span>
                    {active && <Check size={13} />}
                  </button>
                );
              })}
            </div>
          </div>
        )}
      </div>

      <button
        type="button"
        className="db-clear-page"
        disabled={readOnly || currentKeys === 0}
        title={
          readOnly
            ? "只读模式已开启，无法清空数据库"
            : currentKeys === 0
              ? "当前数据库为空"
              : `清空 db${currentDb} 的全部键`
        }
        aria-label={`清空 db${currentDb}`}
        onClick={() => {
          setOpen(false);
          setClearOpen(true);
        }}
      >
        <Trash2 size={13} />
        <span>清空</span>
      </button>

      {clearOpen && (
        <ClearDatabaseDialog
          db={currentDb}
          keys={currentKeys}
          onClose={() => setClearOpen(false)}
        />
      )}
    </div>
  );
}
