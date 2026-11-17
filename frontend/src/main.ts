import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHistory } from "vue-router";
import App from "./App.vue";
import "./style.css";
const Empty = { template: "<span></span>" };
const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/files/root" },
    { path: "/files/:id", component: Empty },
    { path: "/recent", component: Empty },
    { path: "/shares", component: Empty },
    { path: "/trash", component: Empty },
    { path: "/settings", component: Empty },
    { path: "/s/:token", component: Empty },
  ],
});
createApp(App).use(createPinia()).use(router).mount("#app");
