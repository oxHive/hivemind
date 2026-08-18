import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import '@oxhive/ui/tokens.css'
import './style.css'

createApp(App).use(createPinia()).mount('#app')
