exports.transition = (state, event) => {
  if (event === "collapse") return "idle";
  if (event === "pin") return "pinned";
  if (event === "enter" && state === "idle") return "preview";
  if (event === "leave" && state === "preview") return "idle";
  return state;
};
