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

  const request = async (message) => {
    const { id, method } = message;
    const params = isRecord(message.params) ? message.params : {};
    switch (method) {
      case "ui/initialize":
        return reply(id, {
          protocolVersion: APPS_VERSION,
          hostInfo: config.host,
          hostCapabilities: { serverTools: {}, serverResources: {} },
          hostContext: context(),
        });
      case "ui/request-display-mode":
        return reply(id, { mode: "fullscreen" });
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

  window.addEventListener("message", (event) => {
    if (event.source !== frame.contentWindow || event.origin !== config.sandbox) return;
    const message = event.data;
    if (!isRecord(message) || message.jsonrpc !== "2.0" || typeof message.method !== "string") return;
    if (!("id" in message)) {
      if (message.method === "ui/notifications/sandbox-proxy-ready") void load();
      else if (message.method === "ui/notifications/initialized") initialized = true;
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
  });
})();
