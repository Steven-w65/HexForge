import { createApp } from 'vue'
import App from './App.vue'
import TemplateEditorWindow from './components/TemplateEditorWindow.vue'
import './styles/theme.css'

createApp(new URLSearchParams(window.location.search).get('view') === 'template-editor' ? TemplateEditorWindow : App).mount('#app')
