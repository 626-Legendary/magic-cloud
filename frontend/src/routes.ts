import type { RouteRecordRaw } from "vue-router";

const Empty = { template: "<span></span>" };
export const routes: RouteRecordRaw[] = [
  { path: "/", redirect: "/files/root" },
  { path: "/files/:id", name: "files", component: Empty },
  { path: "/recent", name: "recent", component: Empty },
  { path: "/shares", name: "shares", component: Empty },
  { path: "/trash", name: "trash", component: Empty },
  { path: "/settings", name: "settings", component: Empty },
  { path: "/s/:token", name: "public", component: Empty },
  { path: "/:pathMatch(.*)*", name: "not-found", component: Empty },
];
