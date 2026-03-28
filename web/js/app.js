// ── App Store (glue, toast, undo, nav, history, utilities) ────────────────
document.addEventListener("alpine:init", () => {
  Alpine.store("app", {
    currentView: "collections",
    showSidebar: false,
    _skipPush: false, // prevents circular hash ↔ state updates

    // ── Navigation with history ─────────────────────────────────────
    navigate(view, collection) {
      const col = Alpine.store("collections");
      if (collection !== undefined && collection !== col.activeCollection) {
        col.activeCollection = collection;
        if (collection) {
          const data = Alpine.store("data");
          data.resetBrowse();
          data.loadKeys();
          col.loadSize();
        }
      }
      this.currentView = view;
      this.showSidebar = false;
      this._pushHash();
    },

    _pushHash() {
      if (this._skipPush) return;
      const col = Alpine.store("collections").activeCollection;
      const hash = col
        ? `#/${encodeURIComponent(col)}/${this.currentView}`
        : "#/";
      if (window.location.hash !== hash) {
        history.pushState(null, "", hash);
      }
    },

    _restoreFromHash() {
      const hash = window.location.hash || "#/";
      const parts = hash.replace("#/", "").split("/").map(decodeURIComponent);
      const col = parts[0] || "";
      const view = parts[1] || "";
      const collections = Alpine.store("collections");
      const auth = Alpine.store("auth");

      // Not logged in — nothing to restore
      if (!auth.isLoggedIn) return;

      this._skipPush = true;
      if (col && col !== collections.activeCollection) {
        // Check if this collection exists in the list
        const exists = collections.list.some((c) => c.name === col);
        if (exists) {
          collections.activeCollection = col;
          const data = Alpine.store("data");
          data.resetBrowse();
          data.loadKeys();
          collections.loadSize();
          this.currentView = view || "browse";
        } else {
          // Collection not found — go to landing
          this.currentView = "collections";
        }
      } else if (col && col === collections.activeCollection && view) {
        this.currentView = view;
      } else if (!col) {
        collections.activeCollection = "";
        this.currentView = "collections";
      }
      this._skipPush = false;
    },

    // ── Toast ────────────────────────────────────────────────────────
    toasts: [],
    _toastId: 0,

    toast(msg, type = "success") {
      const id = ++this._toastId;
      this.toasts.push({ id, msg, type });
      setTimeout(() => {
        this.toasts = this.toasts.filter((t) => t.id !== id);
      }, 4000);
    },

    // ── Undo ─────────────────────────────────────────────────────────
    undoAction: null,
    undoProgress: 100,
    _undoTimer: null,
    _undoTick: null,

    scheduleUndo(msg, undoFn, commitFn, durationMs = 5000) {
      this._clearUndo();
      this.undoAction = { msg, undoFn, commitFn };
      this.undoProgress = 100;
      const start = Date.now();
      this._undoTick = setInterval(() => {
        this.undoProgress = Math.max(
          0,
          100 - ((Date.now() - start) / durationMs) * 100,
        );
      }, 50);
      this._undoTimer = setTimeout(() => {
        const action = this.undoAction;
        this._clearUndo();
        if (action?.commitFn) action.commitFn();
      }, durationMs);
    },

    doUndo() {
      const action = this.undoAction;
      this._clearUndo();
      if (action?.undoFn) action.undoFn();
    },

    _clearUndo() {
      if (this._undoTimer) clearTimeout(this._undoTimer);
      if (this._undoTick) clearInterval(this._undoTick);
      this._undoTimer = null;
      this._undoTick = null;
      this.undoAction = null;
      this.undoProgress = 100;
    },

    // ── Auto-refresh ─────────────────────────────────────────────────
    _autoRefreshTimer: null,
    autoRefreshEnabled: false,

    startAutoRefresh() {
      this.stopAutoRefresh();
      this.autoRefreshEnabled = true;
      this._autoRefreshTimer = setInterval(() => {
        const col = Alpine.store("collections");
        const data = Alpine.store("data");
        if (
          col.activeCollection &&
          this.currentView === "browse" &&
          !data.browseLoading
        ) {
          data.loadKeys();
        }
      }, 5000);
    },

    stopAutoRefresh() {
      this.autoRefreshEnabled = false;
      if (this._autoRefreshTimer) {
        clearInterval(this._autoRefreshTimer);
        this._autoRefreshTimer = null;
      }
    },

    // ── Utilities ────────────────────────────────────────────────────
    syntaxHighlight(obj) {
      const json = typeof obj === "string" ? obj : JSON.stringify(obj, null, 2);
      return json.replace(
        /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g,
        (match) => {
          let cls = "json-number";
          if (/^"/.test(match)) {
            if (/:$/.test(match)) {
              return `<span class="json-key">${match.slice(0, -1)}</span>:`;
            }
            cls = "json-string";
          } else if (/true|false/.test(match)) {
            cls = "json-bool";
          } else if (/null/.test(match)) {
            cls = "json-null";
          }
          return `<span class="${cls}">${match}</span>`;
        },
      );
    },

    previewValue(value) {
      const s = JSON.stringify(value);
      return s.length > 60 ? s.slice(0, 60) + "…" : s;
    },

    handleTab(event) {
      event.preventDefault();
      const ta = event.target;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      ta.value = ta.value.substring(0, start) + "  " + ta.value.substring(end);
      ta.selectionStart = ta.selectionEnd = start + 2;
      ta.dispatchEvent(new Event("input"));
    },

    formatBytes(bytes) {
      if (!bytes || bytes === 0) return "0 B";
      const units = ["B", "KB", "MB", "GB"];
      const i = Math.floor(Math.log(bytes) / Math.log(1024));
      return (
        (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + " " + units[i]
      );
    },

    // Replace the access key in a string with a masked version
    maskKey(str) {
      const key = Alpine.store("auth").accessKey;
      if (!key || key.length < 4) return str;
      const masked = key.slice(0, 4) + "••••••••";
      return str.replaceAll(key, masked);
    },

    // ── Usage examples ───────────────────────────────────────────────
    _baseUrl() {
      return `${window.location.origin}${Alpine.store("auth").apiBase}`;
    },

    curlPut() {
      const c = Alpine.store("collections").activeCollection;
      const k = Alpine.store("auth").accessKey;
      return `curl -X PUT \\
  ${this._baseUrl()}/api/${c}/my-key \\
  -H "X-SECRET-KEY: ${k}" \\
  -H "Content-Type: application/json" \\
  -d '{"name": "Alice", "age": 30}'`;
    },

    curlGet() {
      const c = Alpine.store("collections").activeCollection;
      const k = Alpine.store("auth").accessKey;
      return `curl ${this._baseUrl()}/api/${c}/my-key \\
  -H "X-SECRET-KEY: ${k}"`;
    },

    curlList() {
      const c = Alpine.store("collections").activeCollection;
      const k = Alpine.store("auth").accessKey;
      return `curl "${this._baseUrl()}/api/${c}?keys=true&order=asc" \\
  -H "X-SECRET-KEY: ${k}"`;
    },

    curlQuery() {
      const c = Alpine.store("collections").activeCollection;
      const k = Alpine.store("auth").accessKey;
      return `curl -X POST ${this._baseUrl()}/api/${c} \\
  -H "X-SECRET-KEY: ${k}" \\
  -H "Content-Type: application/json" \\
  -d '{"query": "$[?@.age > 25]", "keys": true}'`;
    },

    curlDelete() {
      const c = Alpine.store("collections").activeCollection;
      const k = Alpine.store("auth").accessKey;
      return `curl -X DELETE ${this._baseUrl()}/api/${c}/my-key \\
  -H "X-SECRET-KEY: ${k}"`;
    },

    curlSubscribe() {
      const c = Alpine.store("collections").activeCollection;
      return `curl -N ${this._baseUrl()}/api/${c}/_subscribe`;
    },

    jsExample() {
      const c = Alpine.store("collections").activeCollection;
      const k = Alpine.store("auth").accessKey;
      return `const BASE = "${this._baseUrl()}/api/${c}";
const KEY = "${k}";
const headers = {
  "Content-Type": "application/json",
  "X-SECRET-KEY": KEY,
};

// Insert
await fetch(\`\${BASE}/user:1\`, {
  method: "PUT", headers,
  body: JSON.stringify({ name: "Alice", age: 30 }),
});

// Get
const res = await fetch(\`\${BASE}/user:1\`, { headers });
const data = await res.json();

// Query
const results = await fetch(BASE, {
  method: "POST", headers,
  body: JSON.stringify({ query: "$[?@.age > 25]", keys: true }),
}).then(r => r.json());

// Subscribe (SSE)
const es = new EventSource(\`\${BASE}/_subscribe\`);
es.onmessage = (e) => console.log(JSON.parse(e.data));`;
    },
  });
});

// ── Init + History ────────────────────────────────────────────────────────
document.addEventListener("alpine:init", () => {
  // Watch view/collection changes → push hash
  Alpine.effect(() => {
    const view = Alpine.store("app").currentView;
    const col = Alpine.store("collections").activeCollection;

    // Side effects for view changes
    if (view === "backups" && col) Alpine.store("backups").load();
    if (view !== "browse") Alpine.store("app").stopAutoRefresh();

    // Push hash (skipped during popstate restore)
    Alpine.store("app")._pushHash();
  });

  // Listen for back/forward
  window.addEventListener("popstate", () => {
    Alpine.store("app")._restoreFromHash();
  });

  // Auto-login, then restore hash
  try {
    const savedKey = localStorage.getItem("smolkv_key");
    if (savedKey) {
      Alpine.store("auth").accessKey = savedKey;
      // Login, then restore URL state after collections are loaded
      Alpine.store("auth")
        .login()
        .then(() => {
          if (window.location.hash && window.location.hash !== "#/") {
            Alpine.store("app")._restoreFromHash();
          }
        });
    }
  } catch {}
});
