import { api } from "./store";

/*
  The platform's colour picker is a window of its own, so opening one takes
  the focus from Bard. Tell the backend first, so it does not read that as the
  person clicking away and tuck the window they are still using. The backend
  lets go by itself when the focus comes back.
*/
export function holdForNativePickers() {
  document.addEventListener(
    "click",
    (event) => {
      const target = event.target;
      if (target instanceof HTMLInputElement && target.type === "color") {
        void api.holdWindows(true).catch(() => undefined);
      }
    },
    true,
  );
}
