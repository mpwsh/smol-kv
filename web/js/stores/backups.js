// ── Backups Store ─────────────────────────────────────────────────────────
document.addEventListener("alpine:init", () => {
  Alpine.store("backups", {
    list: [],
    loading: false,
    backupInProgress: false,
    restoreInProgress: false,
    showUpload: false,

    async load() {
      const col = Alpine.store("collections");
      if (!col.activeCollection) return;
      this.loading = true;
      const auth = Alpine.store("auth");
      try {
        const res = await fetch(col.collectionUrl("/_backup"), {
          headers: auth.headers(),
        });
        if (res.ok) this.list = await res.json();
      } catch (e) {
        Alpine.store("app").toast(`Failed: ${e.message}`, "error");
      } finally {
        this.loading = false;
      }
    },

    async start() {
      const col = Alpine.store("collections");
      if (!col.activeCollection) return;
      this.backupInProgress = true;
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const res = await fetch(col.collectionUrl("/_backup"), {
          method: "POST",
          headers: auth.headers(),
        });
        if (res.ok) {
          const data = await res.json();
          app.toast("Backup started");
          this._pollStatus("_backup", data.id, "backupInProgress");
        } else {
          app.toast((await res.json()).error || "Backup failed", "error");
          this.backupInProgress = false;
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
        this.backupInProgress = false;
      }
    },

    async startRestore(backupId) {
      const col = Alpine.store("collections");
      if (!col.activeCollection || !backupId) {
        Alpine.store("app").toast("Select a backup to restore", "error");
        return;
      }
      if (!confirm("Restore will overwrite current data. Continue?")) return;
      this.restoreInProgress = true;
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      try {
        const res = await fetch(
          col.collectionUrl(
            `/_restore?backup_id=${encodeURIComponent(backupId)}`,
          ),
          { method: "POST", headers: auth.headers() },
        );
        if (res.ok) {
          const data = await res.json();
          app.toast("Restore started");
          this._pollStatus("_restore", data.id, "restoreInProgress", () =>
            Alpine.store("data").loadKeys(),
          );
        } else {
          app.toast((await res.json()).error || "Restore failed", "error");
          this.restoreInProgress = false;
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
        this.restoreInProgress = false;
      }
    },

    async _pollStatus(endpoint, id, progressKey, onComplete) {
      const col = Alpine.store("collections");
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      const poll = async () => {
        try {
          const res = await fetch(
            col.collectionUrl(
              `/${endpoint}/status?id=${encodeURIComponent(id)}`,
            ),
            { headers: auth.headers() },
          );
          if (res.ok) {
            const status = await res.json();
            if (status.status === "completed") {
              this[progressKey] = false;
              app.toast(
                endpoint === "_backup"
                  ? "Backup completed"
                  : "Restore completed",
              );
              if (endpoint === "_backup") this.load();
              if (onComplete) onComplete();
              return;
            }
            if (status.status === "failed") {
              this[progressKey] = false;
              app.toast(
                status.error ||
                  (endpoint === "_backup"
                    ? "Backup failed"
                    : "Restore failed"),
                "error",
              );
              return;
            }
          }
        } catch {}
        if (this[progressKey]) setTimeout(poll, 2000);
      };
      poll();
    },

    downloadUrl(backup) {
      return backup.url
        ? `${Alpine.store("auth").apiBase}${backup.url}`
        : null;
    },

    async upload(event) {
      const file = event.target.files?.[0];
      const col = Alpine.store("collections");
      if (!file || !col.activeCollection) return;
      const auth = Alpine.store("auth");
      const app = Alpine.store("app");
      const formData = new FormData();
      formData.append("file", file);
      try {
        const res = await fetch(col.collectionUrl("/_backup/upload"), {
          method: "POST",
          headers: auth.authHeaders(),
          body: formData,
        });
        if (res.ok) {
          app.toast("Backup uploaded");
          this.showUpload = false;
          this.load();
        } else {
          app.toast((await res.json()).error || "Upload failed", "error");
        }
      } catch (e) {
        app.toast(`Failed: ${e.message}`, "error");
      }
      event.target.value = "";
    },
  });
});
