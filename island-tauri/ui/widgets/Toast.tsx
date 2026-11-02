// Mounted from a sibling React root. The pinned legacy renderer remains byte-identical.
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useIslandState, dispatch } from "../ipc/island-state";
import { useNotifications } from "../ipc/notifications";
import { notificationsDismiss } from "../ipc/client";
import { NotificationPrivacySettings } from "./NotificationPrivacySettings";
import "../styles/toast.css";
export function Toast() {
  return <><NotificationToast /><NotificationPrivacySettings /></>;
}
function NotificationToast() {
  const state = useIslandState();
  const items = useNotifications();
  const id = useRef<string | null>(null);
  const [target, setTarget] = useState<Element | null>(null);
  const [error, setError] = useState("");
  if (state.mode !== "hover") id.current = null;
  else if (state.hoverCause?.type === "notification") id.current = state.hoverCause.id;
  const notification = items.find((item) => item.id === id.current);
  useEffect(() => {
    const sync = () => setTarget(document.querySelector(".island > .preview"));
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(document.getElementById("root") ?? document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);
  if (!target || !notification || state.mode !== "hover") return null;
  return createPortal(<article className="notification-toast" data-testid="notification-toast" role="status"
    onMouseEnter={() => { dispatch({ type: "pointerEnter" }).catch((e: Error) => setError(e.message)); }}>
    <div className="toast-app">{notification.appName}</div>
    <strong>{notification.title}</strong>
    {notification.body && <p>{notification.body}</p>}
    {notification.progress !== null && <progress value={notification.progress} max={1} />}
    <button aria-label="Dismiss notification" onClick={(e) => { e.stopPropagation(); notificationsDismiss(notification.id).catch((e: Error) => setError(e.message)); }}>Dismiss</button>
    {error && <p role="alert">{error}</p>}
  </article>, target);
}
