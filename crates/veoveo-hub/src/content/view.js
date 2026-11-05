// The Hub's page around a file on the content domain. The frame is in the page; this script
// sizes it and renews the token while the page is open. A refusal or another origin reloads
// the page, which signs in or mints anew.
(() => {
  const RENEW_MS = 15 * 60 * 1000;
  const config = JSON.parse(document.getElementById("view").textContent);
  const frame = document.getElementById("content");
  document.documentElement.style.height = "100%";
  document.body.style.height = "100%";
  document.body.style.margin = "0";
  frame.style.border = "0";
  frame.style.display = "block";
  frame.style.height = "100%";
  frame.style.width = "100%";

  const renew = async () => {
    let answer;
    try {
      answer = await fetch(window.location.pathname + window.location.search, {
        method: "POST",
        credentials: "same-origin",
        cache: "no-store",
      });
    } catch {
      return;
    }
    if (answer.status === 401 || answer.status === 403 || answer.status === 404) {
      window.location.reload();
      return;
    }
    if (!answer.ok) return;
    const { origin } = await answer.json();
    if (origin !== config.origin) window.location.reload();
  };

  window.setInterval(renew, RENEW_MS);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) void renew();
  });
})();
