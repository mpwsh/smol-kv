// ── Live Store (SSE) ──────────────────────────────────────────────────────
document.addEventListener("alpine:init", () => {
  Alpine.store("live", {
    events: [],
    connected: false,
    paused: false,
    maxEvents: 200,
    _source: null,
    _buffer: [],

    connect() {
      const col = Alpine.store("collections");
      if (!col.activeCollection) {
        Alpine.store("app").toast("Select a collection first", "error");
        return;
      }
      this.disconnect();
      const auth = Alpine.store("auth");
      let url = col.collectionUrl("/_subscribe");
      if (auth.accessKey) url += `?key=${encodeURIComponent(auth.accessKey)}`;
      const es = new EventSource(url);
      es.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          const entry = { ts: new Date().toISOString(), data };
          if (this.paused) {
            this._buffer.push(entry);
          } else {
            this.events = [entry, ...this.events].slice(0, this.maxEvents);
          }
          if (data.type === "connected") this.connected = true;
        } catch {}
      };
      es.onerror = () => {
        this.connected = false;
      };
      this._source = es;
    },

    disconnect() {
      if (this._source) {
        this._source.close();
        this._source = null;
      }
      this.connected = false;
    },

    togglePause() {
      if (this.paused) {
        this.events = [...this._buffer, ...this.events].slice(
          0,
          this.maxEvents,
        );
        this._buffer = [];
      }
      this.paused = !this.paused;
    },

    clear() {
      this.events = [];
    },

    reset() {
      this.events = [];
      this.connected = false;
      this.paused = false;
    },
  });
});
