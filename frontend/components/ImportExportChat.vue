<script setup lang="ts" xmlns="http://www.w3.org/1999/html">
import {useMainStore} from "~/store";
import { useUiText } from "~/composables/useUiText";
const props = defineProps(['allowDownloadAll'])
const store = useMainStore()
const { t } = useUiText()
const clickModal = ref(false)
const chatImportRef = ref(null)
const errorMessage = ref(undefined)
const disableUpload = ref(true)
const uploadProgress = ref(0)
const uploadState = ref<'idle' | 'uploading' | 'processing' | 'done' | 'error'>('idle')
const modalClass = computed(() => {
  return {
    'fade show d-block': !!clickModal.value
  }
})

const importChatPath = computed(() => useRuntimeConfig().public.api.importChatById.replace(":chatId", store.chatActive?.chatId?.toString()))

const linkDownload = computed(() => {
  if (props.allowDownloadAll) {
    return useRuntimeConfig().public.api.exportAllChats
  } else {
    return useRuntimeConfig().public.api.exportChatById.replace(":chatId", store.chatActive?.chatId?.toString())
  }
})

const chatName = computed(() => {
  if (props.allowDownloadAll) {
    return "all-chats.zip"
  } else {
    return store.chatActive.chatName + '.zip'
  }
})

function toggleModal() {
  clickModal.value = !clickModal.value
  errorMessage.value = undefined
  if (chatImportRef.value) {
    chatImportRef.value.value = ''
  }
}

async function onFilePicked() {
  if (chatImportRef?.value?.files && chatImportRef?.value?.files[0]) {
    disableUpload.value = false
    uploadProgress.value = 0
    uploadState.value = 'idle'
    errorMessage.value = undefined
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
      store.loading = false
      uploadState.value = 'done'
      chatImportRef.value = null
      store.clearMessages()
    }).catch(e => {
      store.loading = false
      uploadState.value = 'error'
      errorMessage.value = e.message || 'Upload failed'
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

watch(
    () => store.chatActive.chatId,
    (chatId) => {
      disableUpload.value = true
      if (chatImportRef.value) {
        chatImportRef.value.value = ''
      }
    }
)
</script>

<template>
  <div class="chat-option mt-3">

    <button type="button" @click="toggleModal" class="btn btn-outline-primary btn-sm import-trigger" data-bs-toggle="modal">
      {{ t('importExport') }}
    </button>

    <div class="modal import-modal" :class="modalClass" tabindex="-1" v-if="clickModal">
      <div class="modal-dialog">
        <div class="modal-content">
          <div class="modal-header">
            <h5 class="modal-title">{{ t('importExportTitle') }}</h5>
            <button type="button" class="btn-close" data-bs-dismiss="modal" @click="toggleModal"
                    :aria-label="t('close')"></button>
          </div>
          <div class="modal-body">
            <div class="content-surface">
              <div class="alert alert-warning" v-if="errorMessage" role="alert">
                {{ t('failedToImport') }}<br/>
                {{ errorMessage }}
              </div>

              <div class="form-group d-flex justify-content-center mb-3">
                <a class="d-block text-center"
                   :href="linkDownload"
                   :download="chatName"
                >
                  {{ t('getEntireChat') }}
                </a>
              </div>

              <div class="form-group mb-3 pt-4 border-top border-2" v-if="!allowDownloadAll">
                <p class="helper">{{ t('actionsApplyCurrentChat') }}</p>
                <label for="formFileSm" class="form-label">{{ t('importMessagesToChat') }}</label>
                <input class="form-control form-control-sm"
                       @change="onFilePicked"
                       accept=".zip,.txt"
                       id="formFileSm"
                       ref="chatImportRef"
                       type="file">
              </div>
              <div class="btn-group" role="group" v-if="!allowDownloadAll">
                <button type="button" :disabled="disableUpload" @click="uploadFile"
                        class="btn btn-outline-secondary ml-2">{{ t('upload') }}
                </button>
              </div>
              <div v-if="uploadState !== 'idle'" class="mt-3">
                <div class="progress" v-if="uploadState === 'uploading' || uploadState === 'processing'">
                  <div class="progress-bar" role="progressbar" :style="{ width: uploadProgress + '%' }">
                    {{ uploadProgress }}%
                  </div>
                </div>
                <small class="text-muted" v-if="uploadState === 'uploading'">Uploading archive...</small>
                <small class="text-muted" v-else-if="uploadState === 'processing'">Upload complete. Processing import on server...</small>
                <small class="text-success" v-else-if="uploadState === 'done'">Import finished.</small>
              </div>
            </div>


          </div>
          <div class="modal-footer">
            <button type="button" @click="toggleModal" class="btn btn-secondary" data-bs-dismiss="modal">{{ t('close') }}</button>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
.import-trigger {
  border-radius: var(--radius-pill);
}

.import-modal {
  background:
    radial-gradient(circle at 18% 18%, rgba(255, 255, 255, 0.14), transparent 45%),
    rgba(2, 6, 23, 0.65);
  backdrop-filter: blur(2px);
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1050;
}

.modal-dialog {
  max-width: 520px;
  width: calc(100% - 2rem);
}

.modal-content {
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  box-shadow: var(--shadow-md);
  overflow: hidden;
}

.modal-header {
  border-bottom: 1px solid var(--color-border-soft);
  background: linear-gradient(180deg, rgba(248, 249, 251, 0.9), rgba(248, 249, 251, 0.6));
  padding: 1.1rem 1.2rem 0.85rem;
}

.btn-close:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
  border-radius: var(--radius-pill);
}

.modal-title {
  font-weight: 600;
  letter-spacing: -0.01em;
}

.modal-body {
  padding: 1rem 1.2rem 0.8rem;
}

.content-surface {
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-soft);
  background: #fff;
  padding: 1rem;
}

.helper {
  color: var(--color-text-muted-dark);
  margin-bottom: 0.75rem;
}

.modal-footer {
  border-top: 1px solid var(--color-border-soft);
  padding: 0.8rem 1.2rem 1.1rem;
}

.modal-footer .btn {
  border-radius: var(--radius-pill);
}
</style>
