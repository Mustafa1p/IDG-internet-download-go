// Validate the final form in the extension's isolated content-script world.
// Only explicit download forms containing hidden fields may be handed off.
globalThis.__idgReadDownloadForm = form => {
  if (form?.tagName !== "FORM" || form.method.toLowerCase() !== "post") return null;
  if (form.enctype && form.enctype !== "application/x-www-form-urlencoded") return null;
  let url;
  try { url = new URL(form.action); } catch { return null; }
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) return null;
  const tokenEndpoint = /^[a-z0-9-]+\.safefilehosting\.com$/i.test(url.hostname) && url.pathname === "/files/download.php";
  if (!tokenEndpoint && !/^(?:download[-_]?form|frmDownload)$/i.test(form.id)) return null;
  const fields = new URLSearchParams();
  for (const input of form.elements) {
    if (input.disabled || !input.name) continue;
    if (input.tagName === "BUTTON" || ["submit", "button", "image", "reset"].includes(input.type)) continue;
    // Uploads, login forms and editable user input stay with the website.
    if (input.tagName !== "INPUT" || input.type !== "hidden") return null;
    fields.append(input.name, input.value);
  }
  const filename = fields.get("filename") || fields.get("file_name");
  if (!filename || !/\.(?:exe|msi|dmg|pkg|deb|rpm|apk|zip|rar|7z|tar|gz|bz2|xz|cab|iso|img|bin|pdf|docx?|xlsx?|pptx?|mp3|mp4|avi|mkv|mov|wmv|flv|webm|m4v|m4a|wav|ogg|flac|aac|torrent|eml|txt|csv)$/i.test(filename)) return null;
  if (/^[a-z0-9-]+\.safefilehosting\.com$/i.test(url.hostname) && url.pathname === "/files/download.php" && !fields.get("token")) return null;
  const post_data = fields.toString();
  return post_data.length <= 65536 ? { url: url.href, filename, post_data } : null;
};
