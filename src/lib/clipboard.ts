export async function copyText(
  text: string,
  clipboard: Clipboard | undefined = globalThis.navigator?.clipboard,
  documentRef: Document | undefined = globalThis.document,
): Promise<void> {
  if (clipboard) {
    try {
      await clipboard.writeText(text);
      return;
    } catch {
      // Tauri's WKWebView may reject the Clipboard API even when it is present.
    }
  }

  if (!documentRef) {
    throw new Error("Clipboard access is unavailable.");
  }

  const textarea = documentRef.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  documentRef.body.appendChild(textarea);
  textarea.select();

  try {
    if (!documentRef.execCommand("copy")) {
      throw new Error("The clipboard rejected the copy request.");
    }
  } finally {
    textarea.remove();
  }
}
