// The download site calls form.submit(), which does not emit a submit event.
// Run in the page world so IDG can preserve the final token-bearing POST.
(() => {
  const originalSubmit = HTMLFormElement.prototype.submit;
  const pending = new WeakSet();
  const accepted = new WeakSet();
  function handoff(form) {
    if (!(form instanceof HTMLFormElement) || !/^(?:download[-_]?form|frmDownload)$/i.test(form.id) || form.method.toLowerCase() !== "post") return false;
    if (pending.has(form) || accepted.has(form)) return true;
    pending.add(form);
    const result = event => {
      event.stopImmediatePropagation();
      pending.delete(form);
      if (event.detail === "accepted") accepted.add(form);
      else originalSubmit.call(form);
    };
    form.addEventListener("idg-post-result", result, { once: true });
    const handled = !form.dispatchEvent(new CustomEvent("idg-post-submit", { bubbles: true, cancelable: true }));
    if (!handled) {
      form.removeEventListener("idg-post-result", result);
      pending.delete(form);
    }
    return handled;
  }
  HTMLFormElement.prototype.submit = function () {
    if (!handoff(this)) return originalSubmit.call(this);
  };
  document.addEventListener("submit", event => {
    if (!event.defaultPrevented && handoff(event.target)) event.preventDefault();
  }, true);
})();
