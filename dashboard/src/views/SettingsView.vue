<script setup>
import ServerSection from '../components/settings/ServerSection.vue'
import SyncSection from '../components/settings/SyncSection.vue'
import DataSection from '../components/settings/DataSection.vue'
import DangerSection from '../components/settings/DangerSection.vue'
</script>

<template>
  <div class="flex-1 overflow-y-auto px-8 py-8 max-w-xl">
    <h2 class="mb-8 font-medium" style="font-size:16px; color:var(--hm-text-primary)">Settings</h2>
    <div class="flex flex-col gap-10">
      <ServerSection />
      <SyncSection />
      <DataSection />
      <DangerSection />
    </div>
  </div>
</template>
