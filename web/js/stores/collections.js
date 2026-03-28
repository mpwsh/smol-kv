// ── Collections Store ──────────────────────────────────────────────────────
document.addEventListener("alpine:init", () => {
  Alpine.store("collections", {
    list: [],
    activeCollection: "",
    collectionSize: null,
    newName: "",
    newTtl: "",
    createLoading: false,
    showCreateForm: false,

    setList(data) {
      this.list = data.map((c) => ({
        name: c.name,
        internalName: c.internal_name,
      }));
    },

    reset() {
      this.list = [];
      this.activeCollection = "";
      this.collectionSize = null;
    },

    async refresh() {
      const auth = Alpine.store("auth");
      try {
        const res = await fetch(`${auth.apiBase}/api/_collections`, {
          headers: auth.headers(),
        });
        if (res.ok) this.setList(await res.json());
      } catch {}
    },

    async create() {
      const name = this.newName.trim();
      if (!name) return;
      this.createLoading = true;
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        let url = `${auth.apiBase}/api/${encodeURIComponent(name)}`;
        const ttl = parseInt(this.newTtl);
        if (ttl > 0) url += `?ttl=${ttl}`;
        const res = await fetch(url, {
          method: "PUT",
          headers: auth.headers(),
        });
        const data = await res.json();
        if (res.ok) {
          app.toast(`Created: ${name}`);
          this.newName = "";
          this.newTtl = "";
          this.showCreateForm = false;
          await this.refresh();
          await this.select(name);
        } else {
          app.toast(data.error || data || `HTTP ${res.status}`, "error");
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
      } finally {
        this.createLoading = false;
      }
    },

    async drop(name) {
      if (!confirm(`Drop collection "${name}"? This cannot be undone.`)) return;
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const res = await fetch(
          `${auth.apiBase}/api/${encodeURIComponent(name)}`,
          { method: "DELETE", headers: auth.headers() },
        );
        if (res.ok) {
          app.toast(`Deleted: ${name}`);
          this.list = this.list.filter((c) => c.name !== name);
          if (this.activeCollection === name) {
            this.activeCollection = "";
            Alpine.store("data").reset();
            app.navigate("collections", "");
          }
        } else {
          const data = await res.json();
          app.toast(data.error || `HTTP ${res.status}`, "error");
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
      }
    },

    async select(name) {
      Alpine.store("app").navigate("browse", name);
    },

    collectionUrl(path = "") {
      const auth = Alpine.store("auth");
      return `${auth.apiBase}/api/${encodeURIComponent(this.activeCollection)}${path}`;
    },

    async loadSize() {
      if (!this.activeCollection) return;
      const auth = Alpine.store("auth");
      try {
        const res = await fetch(this.collectionUrl("/_size"), {
          headers: auth.headers(),
        });
        if (res.ok) this.collectionSize = await res.json();
      } catch {}
    },

    async compact() {
      if (!this.activeCollection) return;
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const res = await fetch(this.collectionUrl("/_compact"), {
          method: "POST",
          headers: auth.headers(),
        });
        if (res.ok) {
          app.toast("Compaction triggered");
          this.loadSize();
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
      }
    },
  });
});
