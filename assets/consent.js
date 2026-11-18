// Google Analytics (GA4) only after explicit consent; nothing is sent to Google before "Accept".
// The choice is stored in localStorage ("cg-consent" = "granted" | "denied"); the footer link reopens the banner.
(function () {
  var ID = "G-JF41PLPF3K", KEY = "cg-consent";
  var hu = (document.documentElement.lang || "").slice(0, 2) === "hu";
  var T = hu ? {
    title: "Segíts jobbá tenni a callgraph-ot",
    text: "A Google Analytics megmutatja, melyik leírást olvassátok, és honnan érkeztek — ebből döntjük el, melyik nyelvet és funkciót fejlesszük tovább. Hirdetésre nem használjuk, IP-címet nem tárolunk.",
    yes: "Rendben, segítek", no: "Most nem", more: "Részletek"
  } : {
    title: "Help us make callgraph better",
    text: "Google Analytics shows which guides you read and where you come from — that is how we decide which language and feature to improve next. No advertising, no IP addresses stored.",
    yes: "Sure, count me in", no: "Not now", more: "Details"
  };
  function get() { try { return localStorage.getItem(KEY); } catch (e) { return null; } }
  function set(v) { try { localStorage.setItem(KEY, v); } catch (e) {} }
  var loaded = false;
  function load() {
    if (loaded) return; loaded = true;
    window.dataLayer = window.dataLayer || [];
    window.gtag = function () { window.dataLayer.push(arguments); };
    gtag("consent", "default", { ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied", analytics_storage: "granted" });
    gtag("js", new Date());
    gtag("config", ID, { allow_google_signals: false, allow_ad_personalization_signals: false });
    var s = document.createElement("script"); s.async = true;
    s.src = "https://www.googletagmanager.com/gtag/js?id=" + ID; document.head.appendChild(s);
  }
  function clearGa() {
    document.cookie.split(";").forEach(function (c) {
      var n = c.split("=")[0].trim();
      if (/^_ga/.test(n)) ["", "; domain=." + location.hostname.replace(/^www\./, "")].forEach(function (d) {
        document.cookie = n + "=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/" + d;
      });
    });
  }
  var bar;
  function show() {
    if (bar) { bar.hidden = false; return; }
    bar = document.createElement("div"); bar.className = "consent"; bar.setAttribute("role", "dialog");
    bar.setAttribute("aria-label", hu ? "Süti-beállítás" : "Cookie settings");
    var h = document.createElement("strong"); h.className = "consent-title"; h.textContent = T.title;
    var p = document.createElement("p"); p.textContent = T.text + " ";
    var a = document.createElement("a"); a.href = "#privacy"; a.textContent = T.more; p.appendChild(a);
    var y = document.createElement("button"); y.type = "button"; y.className = "consent-yes"; y.textContent = T.yes;
    var n = document.createElement("button"); n.type = "button"; n.className = "consent-no"; n.textContent = T.no;
    y.addEventListener("click", function () { set("granted"); bar.hidden = true; load(); });
    n.addEventListener("click", function () { var was = get() === "granted"; set("denied"); bar.hidden = true; clearGa(); if (was) location.reload(); });
    var b = document.createElement("div"); b.className = "consent-btns"; b.appendChild(n); b.appendChild(y);
    var t = document.createElement("div"); t.className = "consent-text"; t.appendChild(h); t.appendChild(p);
    bar.appendChild(t); bar.appendChild(b); document.body.appendChild(bar); y.focus({ preventScroll: true });
  }
  document.addEventListener("click", function (e) {
    var t = e.target.closest && e.target.closest("[data-consent]");
    if (t) { e.preventDefault(); show(); }
  });
  var v = get();
  if (v === "granted") load(); else if (v !== "denied") show();
})();
