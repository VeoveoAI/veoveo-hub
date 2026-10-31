"use strict";
// The Hub's MCP Apps host page (MCP Apps 2026-01-26, web host). It renders one App in
// its sandbox proxy, on the sandbox origin, and answers the App's requests: the host's
// own, and through the Hub's bridge, which runs them as the signed-in person. It accepts
// a message only from the proxy frame it created, at the sandbox origin.
(() => {
  const APPS_VERSION = "2026-01-26";
  const METHOD_NOT_FOUND = -32601;
  const INVALID_PARAMS = -32602;
  const INTERNAL_ERROR = -32603;
  const config = JSON.parse(document.getElementById("app").textContent);
  const bridgePath = window.location.pathname;
  const taskMethods = new Set(["tasks/get", "tasks/update", "tasks/cancel"]);
  const bridged = new Set(["tools/call", "resources/read", ...taskMethods]);
  // The Tasks this view's calls created: the only Tasks it may ask about.
  const tasks = new Set();
  // The view's open listens, by request id: aborting one ends its stream at the Hub.
  const listens = new Map();
  // What reached the view's listens before it sent ui/notifications/initialized.
  const held = [];
  const scheme = window.matchMedia("(prefers-color-scheme: dark)");
  let initialized = false;
  let loading = false;

  document.documentElement.style.height = "100%";
  Object.assign(document.body.style, { margin: "0", height: "100%", overflow: "hidden" });
  const notice = document.createElement("p");
  notice.setAttribute("role", "status");
  Object.assign(notice.style, { display: "none", margin: "0", padding: "0.75rem 1rem", font: "14px system-ui, sans-serif" });
  const frame = document.createElement("iframe");
  frame.setAttribute("sandbox", "allow-scripts allow-same-origin");
  frame.setAttribute("referrerpolicy", "no-referrer");
  frame.title = config.title;
  Object.assign(frame.style, { display: "block", border: "0", width: "100%", height: "100%" });
  frame.src = config.proxy;
  document.body.append(notice, frame);

  const show = (text) => {
    notice.textContent = text;
    notice.style.display = "block";
  };
  const send = (message) => {
    if (frame.contentWindow !== null) frame.contentWindow.postMessage(message, config.sandbox);
  };
  const reply = (id, result) => send({ jsonrpc: "2.0", id, result });
  const fail = (id, code, message) => send({ jsonrpc: "2.0", id, error: { code, message } });
  const notify = (method, params) => {
    if (initialized) send({ jsonrpc: "2.0", method, params });
  };
  const isRecord = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
  const context = () => ({
    theme: scheme.matches ? "dark" : "light",
    displayMode: "fullscreen",
    availableDisplayModes: ["fullscreen"],
    containerDimensions: { width: frame.clientWidth, height: frame.clientHeight },
    locale: navigator.language,
    timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    platform: "web",
  });

  // One request through the Hub's bridge; its JSON-RPC response, or an error object.
  const bridge = async (method, params) => {
    let response;
    try {
      response = await fetch(bridgePath, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params: params ?? {} }),
        credentials: "same-origin",
        cache: "no-store",
        redirect: "error",
      });
    } catch {
      return { error: { code: INTERNAL_ERROR, message: "The Hub could not be reached." } };
    }
    if (response.status === 401) {
      show("You are signed out. Reload the page to sign in again.");
      return { error: { code: INTERNAL_ERROR, message: "The person is signed out." } };
    }
    if (!response.ok) {
      return { error: { code: INTERNAL_ERROR, message: "The Hub could not complete this request." } };
    }
    try {
      const message = await response.json();
      if (isRecord(message) && (isRecord(message.error) || "result" in message)) return message;
    } catch {
      // A malformed answer is the Hub's failure, reported below.
    }
    return { error: { code: INTERNAL_ERROR, message: "The Hub answered malformed JSON-RPC." } };
  };

  const load = async () => {
    if (loading) return;
    loading = true;
    const answer = await bridge("resources/read", { uri: config.uri });
    const contents = isRecord(answer.result) && Array.isArray(answer.result.contents) ? answer.result.contents : [];
    const view = contents.find((content) =>
      isRecord(content) && content.uri === config.uri && typeof content.text === "string" &&
      typeof content.mimeType === "string" && content.mimeType.startsWith("text/html"));
    if (view === undefined) {
      show(answer.error?.message ?? "The App sent no HTML view.");
      return;
    }
    send({
      jsonrpc: "2.0",
      method: "ui/notifications/sandbox-resource-ready",
      params: { html: view.text, sandbox: "allow-scripts", csp: config.csp },
    });
  };

  // What the view's listens deliver: unchanged, and only after the view initialized.
  const deliver = (message) => {
    if (initialized) send(message);
    else held.push(message);
  };
  const failure = (id, message) => ({ jsonrpc: "2.0", id, error: { code: INTERNAL_ERROR, message } });

  // A view's subscriptions/listen, on the bridge's event stream: each message of the
  // stream reaches the view as the server sent it, the listen's final response last.
  // Aborting the fetch ends the listen at the Hub and at the App's server.
  const listen = async (id, params) => {
    if (listens.has(id)) return fail(id, INVALID_PARAMS, "That listen is already open.");
    const control = new AbortController();
    listens.set(id, control);
    let answered = false;
    const finish = (message) => {
      if (answered) return;
      answered = true;
      deliver(message);
    };
    try {
      const response = await fetch(bridgePath, {
        method: "POST",
        headers: { "content-type": "application/json", accept: "text/event-stream, application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id, method: "subscriptions/listen", params }),
        credentials: "same-origin",
        cache: "no-store",
        redirect: "error",
        signal: control.signal,
      });
      if (response.status === 401) {
        show("You are signed out. Reload the page to sign in again.");
        return finish(failure(id, "The person is signed out."));
      }
      if (response.status === 429) return finish(failure(id, "Too many live listens are open in this session."));
      if (!response.ok) return finish(failure(id, "The Hub could not complete this request."));
      if (!(response.headers.get("content-type") ?? "").startsWith("text/event-stream")) {
        const message = await response.json();
        return finish(isRecord(message) && message.id === id ? message : failure(id, "The Hub answered malformed JSON-RPC."));
      }
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = "";
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer = (buffer + value).replace(/\r\n?/g, "\n");
        let end;
        while ((end = buffer.indexOf("\n\n")) >= 0) {
          const data = buffer.slice(0, end).split("\n")
            .filter((line) => line.startsWith("data:"))
            .map((line) => line.slice(line.startsWith("data: ") ? 6 : 5))
            .join("\n");
          buffer = buffer.slice(end + 2);
          if (data === "") continue;
          const message = JSON.parse(data);
          if (!isRecord(message) || message.jsonrpc !== "2.0") continue;
          if ("id" in message && ("result" in message || "error" in message)) finish(message);
          else deliver(message);
        }
      }
      finish(failure(id, "The listen ended without an answer; listen again to resume."));
    } catch {
      if (!control.signal.aborted) finish(failure(id, "The Hub could not be reached."));
    } finally {
      listens.delete(id);
    }
  };
  const endListens = () => {
    for (const control of listens.values()) control.abort();
  };

  const request = async (message) => {
    const { id, method } = message;
    const params = isRecord(message.params) ? message.params : {};
    switch (method) {
      case "subscriptions/listen":
        return listen(id, params);
      case "ui/initialize":
        return reply(id, {
          protocolVersion: APPS_VERSION,
          hostInfo: config.host,
          hostCapabilities: { serverTools: {}, serverResources: {} },
          hostContext: context(),
        });
      case "ui/request-display-mode":
        return reply(id, { mode: "fullscreen" });
      case "ui/open-link":
        return openLink(id, params);
      case "ping":
        return reply(id, {});
      default:
        break;
    }
    if (!bridged.has(method)) return fail(id, METHOD_NOT_FOUND, `${method} is not served by this host`);
    if (taskMethods.has(method) && !tasks.has(params.taskId)) {
      return fail(id, INVALID_PARAMS, "This App did not start that Task.");
    }
    const answer = await bridge(method, params);
    if (isRecord(answer.error)) return send({ jsonrpc: "2.0", id, error: answer.error });
    const result = answer.result;
    if (method === "tools/call" && isRecord(result) && result.resultType === "task" && typeof result.taskId === "string") {
      tasks.add(result.taskId);
    }
    return reply(id, result);
  };

  // A view never frames a full web application: a frame nested in the view inherits its
  // sandbox and opaque origin, so it holds no session. The view opens its server's app
  // origin top-level instead, the one target this host opens; other targets fail.
  const openLink = (id, params) => {
    let url;
    try {
      url = new URL(typeof params.url === "string" ? params.url : "");
    } catch {
      return fail(id, INVALID_PARAMS, "ui/open-link needs a URL.");
    }
    if (!config.appOrigin || url.origin !== config.appOrigin) {
      return fail(id, INVALID_PARAMS, "This host opens only the App's own server's app origin.");
    }
    // The page itself goes there: no pop-up to block and no opener to reach back.
    reply(id, {});
    window.location.assign(url.href);
  };

  window.addEventListener("message", (event) => {
    if (event.source !== frame.contentWindow || event.origin !== config.sandbox) return;
    const message = event.data;
    if (!isRecord(message) || message.jsonrpc !== "2.0" || typeof message.method !== "string") return;
    if (!("id" in message)) {
      if (message.method === "ui/notifications/sandbox-proxy-ready") void load();
      else if (message.method === "ui/notifications/initialized") {
        initialized = true;
        for (const message of held.splice(0)) send(message);
      } else if (message.method === "notifications/cancelled" && isRecord(message.params)) {
        // The view ends its listen; a cancelled request is answered no more.
        listens.get(message.params.requestId)?.abort();
      }
      return;
    }
    if (typeof message.id !== "string" && typeof message.id !== "number") return;
    void request(message);
  });
  scheme.addEventListener("change", () => notify("ui/notifications/host-context-changed", { theme: context().theme }));
  window.addEventListener("resize", () =>
    notify("ui/notifications/host-context-changed", { containerDimensions: context().containerDimensions }));
  window.addEventListener("pagehide", () => {
    if (initialized) send({ jsonrpc: "2.0", id: "teardown", method: "ui/resource-teardown", params: {} });
    endListens();
  });
})();
