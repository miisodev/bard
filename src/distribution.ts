/** Distribution owns updates; a Linux download cannot install a Windows payload. */
export function updateDelivery(copy: { platform?: string; channel: string }) {
  if (copy.channel === "store") return { inApp: false, label: "Microsoft Store", hint: "The Microsoft Store keeps Bard up to date." };
  if (copy.channel === "flatpak") return { inApp: false, label: "Flathub", hint: "Your software manager updates Bard." };
  if (copy.platform === "linux") return { inApp: false, label: "Linux download", hint: "New versions are at miiso.dev." };
  return { inApp: true, label: "Direct download", hint: "Checked only when you ask. Nothing installs silently." };
}
