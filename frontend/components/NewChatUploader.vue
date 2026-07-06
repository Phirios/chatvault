<script setup lang="ts">
import {useMainStore} from "~/store";
import { useUiText } from "~/composables/useUiText";

const store = useMainStore()
const { t } = useUiText()
const emit = defineEmits(['update:chats', 'exit:dialog'])
const chatImportRef = ref(null)
const importChatResult = ref({data: null, errorMessage: null})
const fileValid = ref(false)
const chatName = ref(null)
const uploadProgress = ref(0)
const uploadState = ref<'idle' | 'uploading' | 'processing' | 'done' | 'error'>('idle')
const importChatPath = computed(() => {
  if (chatName.value != null) {
    return useRuntimeConfig().public.api.importChatByName.replace(":chatName", chatName.value);
  }
})

const chatNameValid = computed(() => chatName.value !== null && chatName.value.trim() !== '')

const disableUpload = computed(() => {
  return !chatNameValid.value || fileValid.value === false
})

async function onFilePicked() {
  if (chatImportRef?.value?.files && chatImportRef?.value?.files[0]) {
    fileValid.value = true
    uploadProgress.value = 0
    uploadState.value = 'idle'
    importChatResult.value.errorMessage = null
    const inferredName = inferChatName(chatImportRef.value.files[0].name)
    if (!chatName.value && inferredName) {
      chatName.value = inferredName
    }
  }
}

async function uploadFile() {
  if (chatImportRef?.value?.files && chatImportRef?.value?.files[0]) {
    store.loading = true
    const form = new FormData()
    form.append("file", chatImportRef.value.files[0]);
    uploadState.value = 'uploading'
    uploadProgress.value = 0

    uploadWithProgress(importChatPath.value, form, (percent) => {
      uploadProgress.value = percent
      if (percent >= 100) {
        uploadState.value = 'processing'
      }
    }).then(() => {
      uploadState.value = 'done'
      store.loading = false
      emit('update:chats')
      chatImportRef.value.value = ''
    }).catch(e => {
      store.loading = false
      uploadState.value = 'error'
      importChatResult.value.errorMessage = e.message || 'Upload failed'
    })
  }
}

function uploadWithProgress(url: string, form: FormData, onProgress: (percent: number) => void): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open('POST', url)
    request.upload.onprogress = (event) => {
      if (event.lengthComputable) {
        onProgress(Math.round((event.loaded / event.total) * 100))
      }
    }
    request.onload = () => {
      if (request.status >= 200 && request.status < 300) {
        resolve()
      } else {
        reject(new Error(request.responseText || `Upload failed with status ${request.status}`))
      }
    }
    request.onerror = () => reject(new Error('Network error during upload'))
    request.send(form)
  })
}

function inferChatName(fileName: string): string {
  return fileName
      .replace(/\.(zip|txt)$/i, '')
      .replace(/^WhatsApp Chat - /i, '')
      .replace(/^Conversa do WhatsApp com /i, '')
      .trim()
}

function cancel() {
  emit('exit:dialog')
}

</script>

<template>
  <div class="m-auto col-md-3">
    <div class="form-control">
      <div class="alert alert-warning" v-if="importChatResult.errorMessage" role="alert">
        {{ t('failedToImport') }}.<br/>
        {{ importChatResult.errorMessage }}
      </div>

      <div class="form-group">
        <label for="formGroupExampleInput">{{ t('newChatNameLabel') }}</label>
        <input type="text" class="form-control" id="formGroupExampleInput" v-model="chatName" :placeholder="t('newChatNamePlaceholder')">
      </div>

      <div class="form-group mb-3 pt-4 border-top border-2">
        <label for="formFileSm" class="form-label text-black">{{ t('importNewChatLabel') }}</label>
        <input class="form-control form-control-sm"
               @change="onFilePicked"
               accept=".zip,.txt"
               id="formFileSm"
               ref="chatImportRef"
               type="file">
      </div>
      <div v-if="uploadState !== 'idle'" class="my-3">
        <div class="progress" v-if="uploadState === 'uploading' || uploadState === 'processing'">
          <div class="progress-bar" role="progressbar" :style="{ width: uploadProgress + '%' }">
            {{ uploadProgress }}%
          </div>
        </div>
        <small class="text-muted" v-if="uploadState === 'uploading'">Uploading archive...</small>
        <small class="text-muted" v-else-if="uploadState === 'processing'">Upload complete. Processing import on server...</small>
        <small class="text-success" v-else-if="uploadState === 'done'">Import finished.</small>
      </div>
      <div class="btn-group" role="group">
        <button type="button" :disabled="disableUpload" @click="uploadFile"
                class="btn btn-outline-secondary ml-2">{{ t('uploadButton') }}
        </button>
        <button type="button" @click="cancel"
                class="btn btn-outline-secondary ml-2">{{ t('cancelButton') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>

</style>
