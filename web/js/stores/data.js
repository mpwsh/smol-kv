// ── Data Store (browse + query + import + inline key editor) ──────────────
document.addEventListener("alpine:init", () => {
  Alpine.store("data", {
    // ── Browse ──────────────────────────────────────────────────────────
    browseKeys: [],
    browseLoading: false,
    browseSearch: "",
    browseShowCount: 50,
    browseOrder: "asc",

    get filteredKeys() {
      if (!this.browseSearch.trim()) return this.browseKeys;
      const q = this.browseSearch.toLowerCase();
      return this.browseKeys.filter(
        (item) =>
          item.key.toLowerCase().includes(q) ||
          JSON.stringify(item.value).toLowerCase().includes(q),
      );
    },

    async loadKeys() {
      const col = Alpine.store("collections");
      if (!col.activeCollection) return;
      this.browseLoading = true;
      const auth = Alpine.store("auth");
      try {
        const res = await fetch(
          col.collectionUrl(`?keys=true&order=${this.browseOrder}`),
          { headers: auth.headers() },
        );
        if (res.ok) {
          const data = await res.json();
          this.browseKeys = Array.isArray(data) ? data : [];
        } else {
          Alpine.store("app").toast("Failed to load keys", "error");
        }
      } catch (e) {
        Alpine.store("app").toast(`Failed: ${e.message}`, "error");
      } finally {
        this.browseLoading = false;
      }
    },

    resetBrowse() {
      this.browseKeys = [];
      this.browseShowCount = 50;
      this.expandedKey = null;
      this.editorText = "";
      this.editorMode = "view";
      this.selected = [];
    },

    reset() {
      this.resetBrowse();
      this.browseSearch = "";
      this.queryResults = null;
      this.queryHistory = [];
    },

    // ── Multi-select ──────────────────────────────────────────────────
    selected: [],

    isSelected(key) {
      return this.selected.includes(key);
    },

    toggleSelect(key) {
      this.selected = this.selected.includes(key)
        ? this.selected.filter((k) => k !== key)
        : [...this.selected, key];
    },

    // Select/deselect ALL keys (not just visible page)
    selectAll() {
      const allKeys = this.filteredKeys.map((k) => k.key);
      const allSelected =
        allKeys.length > 0 && allKeys.every((k) => this.selected.includes(k));
      this.selected = allSelected ? [] : [...allKeys];
    },

    get allSelected() {
      const allKeys = this.filteredKeys;
      return (
        allKeys.length > 0 &&
        allKeys.every((k) => this.selected.includes(k.key))
      );
    },

    clearSelection() {
      this.selected = [];
    },

    async deleteSelected() {
      if (this.selected.length === 0) return;
      const count = this.selected.length;
      if (
        !confirm(
          `Delete ${count} key${count > 1 ? "s" : ""}? This cannot be undone.`,
        )
      )
        return;
      const col = Alpine.store("collections");
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      let deleted = 0;
      let failed = 0;
      const keys = [...this.selected];
      for (const key of keys) {
        try {
          const res = await fetch(
            col.collectionUrl(`/${encodeURIComponent(key)}`),
            { method: "DELETE", headers: auth.headers() },
          );
          if (res.ok) {
            this.browseKeys = this.browseKeys.filter((k) => k.key !== key);
            if (this.expandedKey === key) this.collapseKey();
            deleted++;
          } else {
            failed++;
          }
        } catch {
          failed++;
        }
      }
      this.selected = [];
      if (deleted) app.toast(`Deleted ${deleted} key${deleted > 1 ? "s" : ""}`);
      if (failed) app.toast(`${failed} failed to delete`, "error");
    },

    // ── Inline key editor ─────────────────────────────────────────────
    expandedKey: null,
    editorText: "",
    editorValid: true,
    editorMode: "view",
    editorSaving: false,

    toggleKey(item) {
      if (this.expandedKey === item.key) {
        this.collapseKey();
      } else {
        this.expandedKey = item.key;
        this.editorText = JSON.stringify(item.value, null, 2);
        this.editorValid = true;
        this.editorMode = "view";
      }
    },

    editKey(item) {
      this.expandedKey = item.key;
      this.editorText = JSON.stringify(item.value, null, 2);
      this.editorValid = true;
      this.editorMode = "edit";
    },

    collapseKey() {
      this.expandedKey = null;
      this.editorText = "";
      this.editorMode = "view";
    },

    expandedValue() {
      if (!this.expandedKey) return null;
      const item = this.browseKeys.find((k) => k.key === this.expandedKey);
      return item ? item.value : null;
    },

    startEdit() {
      const val = this.expandedValue();
      if (val !== null) this.editorText = JSON.stringify(val, null, 2);
      this.editorMode = "edit";
    },

    cancelEdit() {
      this.editorMode = "view";
      const val = this.expandedValue();
      if (val !== null) this.editorText = JSON.stringify(val, null, 2);
      this.editorValid = true;
    },

    validateEditor() {
      try {
        JSON.parse(this.editorText);
        this.editorValid = true;
      } catch {
        this.editorValid = false;
      }
    },

    formatEditor() {
      try {
        this.editorText = JSON.stringify(JSON.parse(this.editorText), null, 2);
        this.editorValid = true;
      } catch {
        this.editorValid = false;
      }
    },

    async saveEditor() {
      if (!this.editorValid || !this.expandedKey) return;
      this.editorSaving = true;
      const col = Alpine.store("collections");
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const parsed = JSON.parse(this.editorText);
        const res = await fetch(
          col.collectionUrl(`/${encodeURIComponent(this.expandedKey)}`),
          {
            method: "PUT",
            headers: auth.headers(),
            body: JSON.stringify(parsed),
          },
        );
        if (res.ok) {
          this.editorMode = "view";
          app.toast(`Saved: ${this.expandedKey}`);
          const idx = this.browseKeys.findIndex(
            (k) => k.key === this.expandedKey,
          );
          if (idx !== -1) this.browseKeys[idx].value = parsed;
          this.editorText = JSON.stringify(parsed, null, 2);
        } else {
          app.toast("Save failed", "error");
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
      } finally {
        this.editorSaving = false;
      }
    },

    async deleteKey(key) {
      const item = this.browseKeys.find((k) => k.key === key);
      if (!item) return;
      const savedValue = JSON.parse(JSON.stringify(item.value));
      this.browseKeys = this.browseKeys.filter((k) => k.key !== key);
      this.selected = this.selected.filter((k) => k !== key);
      if (this.expandedKey === key) this.collapseKey();
      const col = Alpine.store("collections");
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const res = await fetch(
          col.collectionUrl(`/${encodeURIComponent(key)}`),
          { method: "DELETE", headers: auth.headers() },
        );
        if (!res.ok) {
          this._reinsertKey(key, savedValue);
          app.toast("Delete failed on server", "error");
          return;
        }
      } catch (e) {
        this._reinsertKey(key, savedValue);
        app.toast(`Failed: ${e.message}`, "error");
        return;
      }
      app.scheduleUndo(
        `Deleted "${key}"`,
        async () => {
          try {
            const res = await fetch(
              col.collectionUrl(`/${encodeURIComponent(key)}`),
              {
                method: "PUT",
                headers: auth.headers(),
                body: JSON.stringify(savedValue),
              },
            );
            if (res.ok) {
              this._reinsertKey(key, savedValue);
              app.toast(`Restored: ${key}`);
            } else {
              app.toast("Restore failed", "error");
            }
          } catch (e) {
            app.toast(`Restore failed: ${e.message}`, "error");
          }
        },
        null,
      );
    },

    _reinsertKey(key, value) {
      this.browseKeys = [...this.browseKeys, { key, value }].sort((a, b) =>
        a.key.localeCompare(b.key),
      );
    },

    // ── Create key ────────────────────────────────────────────────────
    newKeyName: "",
    newKeyValue: "{\n  \n}",
    newKeyValid: true,
    newKeyCreating: false,
    showNewKeyForm: false,

    validateNewKey() {
      try {
        JSON.parse(this.newKeyValue);
        this.newKeyValid = true;
      } catch {
        this.newKeyValid = false;
      }
    },

    async createKey() {
      const key = this.newKeyName.trim();
      if (!key || !this.newKeyValid) return;
      this.newKeyCreating = true;
      const col = Alpine.store("collections");
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const parsed = JSON.parse(this.newKeyValue);
        const res = await fetch(
          col.collectionUrl(`/${encodeURIComponent(key)}`),
          {
            method: "PUT",
            headers: auth.headers(),
            body: JSON.stringify(parsed),
          },
        );
        if (res.ok) {
          app.toast(`Created: ${key}`);
          this.newKeyName = "";
          this.newKeyValue = "{\n  \n}";
          this.showNewKeyForm = false;
          this.loadKeys();
        } else {
          app.toast("Create failed", "error");
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
      } finally {
        this.newKeyCreating = false;
      }
    },

    // ── Import (with preview) ─────────────────────────────────────────
    showImportModal: false,
    importLoading: false,
    importKeyField: "",
    importData: null, // parsed JSON array held in memory
    importFileName: "",
    importSample: null, // first item for preview
    importFields: [], // top-level field names from first item
    importInserted: 0,
    importTotal: 0,

    loadImportFile(event) {
      const file = event.target.files?.[0];
      if (!file) return;
      this.importFileName = file.name;
      const reader = new FileReader();
      reader.onload = (e) => {
        try {
          const data = JSON.parse(e.target.result);
          if (!Array.isArray(data) || data.length === 0) {
            Alpine.store("app").toast(
              "File must be a non-empty JSON array",
              "error",
            );
            return;
          }
          this.importData = data;
          this.importSample = data[0];
          this.importFields =
            typeof data[0] === "object" && data[0] !== null
              ? Object.keys(data[0])
              : [];
          this.importKeyField = "";
        } catch {
          Alpine.store("app").toast("Invalid JSON file", "error");
        }
      };
      reader.readAsText(file);
      event.target.value = "";
    },

    pickImportField(field) {
      this.importKeyField = this.importKeyField === field ? "" : field;
    },

    cancelImport() {
      this.showImportModal = false;
      this.importData = null;
      this.importSample = null;
      this.importFields = [];
      this.importKeyField = "";
      this.importFileName = "";
      this.importInserted = 0;
      this.importTotal = 0;
    },

    async confirmImport() {
      if (!this.importData || !Alpine.store("collections").activeCollection)
        return;
      this.importLoading = true;
      const col = Alpine.store("collections");
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      const data = this.importData;
      this.importTotal = data.length;
      this.importInserted = 0;
      let failed = 0;
      for (let i = 0; i < data.length; i++) {
        const item = data[i];
        const key =
          this.importKeyField && item[this.importKeyField] != null
            ? String(item[this.importKeyField])
            : `item_${i + 1}`;
        try {
          const res = await fetch(
            col.collectionUrl(`/${encodeURIComponent(key)}`),
            {
              method: "PUT",
              headers: auth.headers(),
              body: JSON.stringify(item),
            },
          );
          if (res.ok) {
            this.importInserted++;
          } else {
            failed++;
          }
        } catch {
          failed++;
        }
      }
      this.importLoading = false;
      app.toast(
        `Imported ${this.importInserted} key${this.importInserted !== 1 ? "s" : ""}${failed ? `, ${failed} failed` : ""}`,
      );
      this.cancelImport();
      this.loadKeys();
    },

    // ── Query ─────────────────────────────────────────────────────────
    queryInput: "",
    queryIncludeKeys: true,
    queryLimit: "",
    queryResults: null,
    queryError: "",
    queryLoading: false,
    queryDuration: null,
    queryHistory: [],

    async runQuery() {
      const col = Alpine.store("collections");
      if (!col.activeCollection || !this.queryInput.trim()) return;
      this.queryLoading = true;
      this.queryError = "";
      this.queryResults = null;
      this.queryDuration = null;
      const start = performance.now();
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const body = {
          query: this.queryInput.trim(),
          keys: this.queryIncludeKeys,
        };
        if (this.queryLimit) body.limit = parseInt(this.queryLimit);
        const res = await fetch(col.collectionUrl(""), {
          method: "POST",
          headers: auth.headers(),
          body: JSON.stringify(body),
        });
        this.queryDuration = Math.round(performance.now() - start);
        const data = await res.json();
        if (res.ok) {
          this.queryResults = data;
          const q = this.queryInput.trim();
          this.queryHistory = [
            q,
            ...this.queryHistory.filter((h) => h !== q),
          ].slice(0, 10);
        } else {
          this.queryError = data.error || JSON.stringify(data);
        }
      } catch (e) {
        this.queryDuration = Math.round(performance.now() - start);
        this.queryError = `Connection failed: ${e.message}`;
      } finally {
        this.queryLoading = false;
      }
    },
  });
});
