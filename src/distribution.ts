/** Distribution owns updates; a Linux download cannot install a Windows payload. */
export function updateDelivery(copy: { platform?: string; channel: string }) {
  if (copy.channel === "store") return { inApp: false, label: "Microsoft Store", hint: "The Microsoft Store keeps Bard up to date." };
  if (copy.channel === "flatpak") return { inApp: false, label: "Flathub", hint: "Update Bard through your software manager or Flatpak." };
  if (copy.platform === "linux") return { inApp: false, label: "Linux download", hint: "Download the latest Linux package from miiso.dev and install it with your package manager." };
  return { inApp: true, label: "Direct download", hint: "Checked only when you ask. Nothing installs silently." };
}
