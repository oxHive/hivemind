<script setup>
import { ref } from 'vue'
import ServerSection from '../components/settings/ServerSection.vue'
import SettingsTabs from '../components/settings/SettingsTabs.vue'
import AppearanceSection from '../components/settings/AppearanceSection.vue'
import SyncSection from '../components/settings/SyncSection.vue'
import TagsSection from '../components/settings/TagsSection.vue'
import DataSection from '../components/settings/DataSection.vue'
import DangerSection from '../components/settings/DangerSection.vue'

const tabs = [
  { id: 'appearance', label: 'Appearance', component: AppearanceSection },
  { id: 'sync', label: 'Sync', component: SyncSection },
  { id: 'tags', label: 'Tags', component: TagsSection },
  { id: 'data', label: 'Data', component: DataSection },
  { id: 'danger', label: 'Danger', component: DangerSection },
]
const activeTab = ref(tabs[0].id)
</script>

<template>
  <div class="flex-1 overflow-y-auto px-8 py-8">
    <div class="max-w-xl">
      <h2 class="mb-8 font-medium" style="font-size:16px; color:var(--hm-text-primary)">Settings</h2>
      <div class="mb-10">
        <ServerSection />
      </div>
      <SettingsTabs :tabs="tabs" :active="activeTab" @select="activeTab = $event" />
      <component :is="tabs.find(t => t.id === activeTab).component" />
    </div>
  </div>
</template>
