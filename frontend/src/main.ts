import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHistory } from "vue-router";
import App from "./App.vue";
import { routes } from "./routes";
import "./style.css";
const router = createRouter({
  history: createWebHistory(),
  sensitive: true,
  routes,
});
createApp(App).use(createPinia()).use(router).mount("#app");
