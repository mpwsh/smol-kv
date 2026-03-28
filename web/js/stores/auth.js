// ── Auth Store ─────────────────────────────────────────────────────────────
document.addEventListener("alpine:init", () => {
  Alpine.store("auth", {
    accessKey: "",
    isLoggedIn: false,
    loginLoading: false,
    showGenerateConfirm: false,
    generatedKey: "",
    apiBase: "",

    headers() {
      const h = { "Content-Type": "application/json" };
      if (this.accessKey) h["X-SECRET-KEY"] = this.accessKey;
      return h;
    },

    authHeaders() {
      const h = {};
      if (this.accessKey) h["X-SECRET-KEY"] = this.accessKey;
      return h;
    },

    generateKey() {
      const chars =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
      const arr = new Uint8Array(32);
      crypto.getRandomValues(arr);
      this.generatedKey = Array.from(arr, (b) => chars[b % chars.length]).join(
        "",
      );
      this.showGenerateConfirm = true;
    },

    confirmGeneratedKey() {
      this.accessKey = this.generatedKey;
      this.generatedKey = "";
      this.showGenerateConfirm = false;
      this.login();
    },

    async login() {
      if (!this.accessKey.trim()) return;
      this.loginLoading = true;
      const app = Alpine.store("app");
      try {
        const res = await fetch(`${this.apiBase}/api/_collections`, {
          headers: this.headers(),
        });
        const collections = Alpine.store("collections");
        if (res.ok) {
          const data = await res.json();
          collections.setList(data);
          app.toast(
            data.length
              ? `Found ${data.length} collection${data.length > 1 ? "s" : ""}`
              : "Connected — no collections yet",
          );
        } else {
          collections.setList([]);
          app.toast("Connected — no collections yet");
        }
        this.isLoggedIn = true;
        app.navigate("collections", "");
        try {
          localStorage.setItem("smolkv_key", this.accessKey);
        } catch {}
      } catch (e) {
        app.toast(`Connection failed: ${e.message}`, "error");
      } finally {
        this.loginLoading = false;
      }
    },

    logout() {
      Alpine.store("live").disconnect();
      this.accessKey = "";
      this.isLoggedIn = false;
      Alpine.store("collections").reset();
      Alpine.store("data").reset();
      Alpine.store("live").reset();
      Alpine.store("app").navigate("collections", "");
      history.replaceState(null, "", "#/");
      try {
        localStorage.removeItem("smolkv_key");
      } catch {}
    },

    async copyToClipboard(text, label = "Copied") {
      try {
        await navigator.clipboard.writeText(text);
        Alpine.store("app").toast(label, "success");
      } catch {
        const ta = document.createElement("textarea");
        ta.value = text;
        ta.style.cssText = "position:fixed;opacity:0";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
        Alpine.store("app").toast(label, "success");
      }
    },
  });
});
