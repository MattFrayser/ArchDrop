//=====
// UI
//=====
const uploadArea = document.getElementById('uploadArea');
const fileInput = document.getElementById('fileInput');
const fileList = document.getElementById('fileList');
const uploadBtn = document.getElementById('uploadBtn');
let selectedFiles = [];

// Click upload
uploadArea.addEventListener('click', () => fileInput.click())

// File selected
fileInput.addEventListener('change', (e) => {
    handleFiles(Array.from(e.target.files))
});

// Drag and drop
uploadArea.addEventListener('dragover', (e) => {
    e.preventDefault()
    uploadArea.classList.add('dragover')
});

uploadArea.addEventListener('dragleave', () => {
    uploadArea.classList.remove('dragover')
});

uploadArea.addEventListener('drop', (e) => {
    e.preventDefault()
    uploadArea.classList.remove('dragover')
    handleFiles(Array.from(e.dataTransfer.files))
});

// Handle multiple files
function handleFiles(files) {
    if (!files || files.length === 0) return

    // Add new files to existing selection
    selectedFiles = [...selectedFiles, ...files]

    updateFileList()
}

// Create summary element
function createSummary(fileCount, totalSize) {
    const summary = document.createElement('div')
    summary.className = 'file-list-summary'
    summary.textContent = `${fileCount} files selected • Total: ${formatFileSize(totalSize)}`
    return summary
}

// Update the file list UI
function updateFileList() {
    // Clear existing content
    fileList.innerHTML = ''

    if (selectedFiles.length === 0) {
        fileList.classList.remove('show')
        uploadBtn.classList.remove('show')
        return
    }

    fileList.classList.add('show')
    uploadBtn.classList.add('show')

    // Add each file
    selectedFiles.forEach((file, index) => {
        const fileItem = createFileItem(file, index, {
            showRemoveButton: true,
            onRemove: removeFile,
            initialProgressText: 'Waiting...'
        })
        fileList.appendChild(fileItem)
    })

    // Add summary if multiple files
    if (selectedFiles.length > 1) {
        const totalSize = selectedFiles.reduce((sum, file) => sum + file.size, 0)
        const summary = createSummary(selectedFiles.length, totalSize)
        fileList.appendChild(summary)
    }

    // Update button text
    uploadBtn.textContent = selectedFiles.length === 1 
        ? 'Upload File' 
        : `Upload ${selectedFiles.length} Files`
}

// Remove individual file
function removeFile(index) {
    selectedFiles.splice(index, 1)
    
    if (selectedFiles.length === 0) {
        fileInput.value = ''
    }
    
    updateFileList()
}

//===========
// LOGIC
//==========
async function sendManifest(token, files) {
    const manifest = {
        files: files.map(file => ({
            relative_path: file.webkitRelativePath || file.name,
            size: file.size
        }))
    };

    const clientId = getClientId();
    const url = `/receive/${token}/manifest?clientId=${clientId}`;
    const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(manifest)
    });

    if (!response.ok) {
        throw new Error('Failed to send manifest');
    }

    return await response.json();
}

async function uploadFiles(selectedFiles) {
    if (selectedFiles.length === 0) {
        alert('Please select files')
        return
    }

    const uploadBtn = document.getElementById('uploadBtn')
    uploadBtn.disabled = true

    // Show progress bars for all files
    const fileItems = fileList.querySelectorAll('.file-item')
    fileItems.forEach(item => {
        const progress = item.querySelector('.file-progress')
        if (progress) progress.classList.add('show')
    })

    try {
        const { key } = await getCredentialsFromUrl()
        const token = window.location.pathname.split('/').pop()

        // Send manifest first so server knows total chunks
        console.time('Manifest upload');
        await sendManifest(token, selectedFiles);
        console.timeEnd('Manifest upload');

        await runWithConcurrency(
            selectedFiles.map((file, index) => ({ file, index, fileItem: fileItems[index] })),
            async ({ file, fileItem }) => {
                const relativePath = file.webkitRelativePath || file.name
                
                fileItem.classList.add('uploading')
                try {
                    await uploadFile(file, relativePath,token, key, fileItem)
                    fileItem.classList.remove('uploading')
                    fileItem.classList.add('completed')
                } catch (error) {
                    fileItem.classList.remove('uploading')
                    fileItem.classList.add('error')
                    throw error
                }
            },
            MAX_CONCURRENT_FILES
        )

        const clientId = getClientId()
        await fetch(`/receive/${token}/complete?clientId=${clientId}`, { method: 'POST' })

        uploadBtn.textContent = 'Upload Complete!'

    } catch(error) {
        console.error(error)
        alert(`Upload failed: ${error.message}`)
        uploadBtn.disabled = false
        uploadBtn.textContent = selectedFiles.length === 1 ? 'Retry Upload' : 'Retry Uploads'
    }
}

async function uploadFile(file, relativePath, token, key, fileItem) {
    // each file gets its own nonce
    const fileNonce = crypto.getRandomValues(new Uint8Array(8));
    const totalChunks = Math.ceil(file.size / CHUNK_SIZE)

    console.log(`Uploading: ${relativePath} (${totalChunks} chunks)`);
    console.time(`${relativePath} - chunk 0`);
    console.time(`${relativePath} - total`);

    // Track completed chunks for progress
    let completedChunks = 0

    // Helper function to prepare and upload a single chunk
    const prepareAndUploadChunk = async (chunkIndex) => {
        const start = chunkIndex * CHUNK_SIZE
        const end = Math.min(start + CHUNK_SIZE, file.size)
        const chunkBlob = file.slice(start, end)
        const chunkData = await chunkBlob.arrayBuffer()

        // Encrypt chunk
        const nonce = generateNonce(fileNonce, chunkIndex)
        const encrypted = await crypto.subtle.encrypt(
            { name: 'AES-GCM', iv: nonce },
            key,
            chunkData
        )

        // Create FormData with chunk and metadata
        const formData = new FormData()
        formData.append('chunk', new Blob([encrypted]))
        formData.append('relativePath', relativePath)
        formData.append('fileName', file.name)
        formData.append('chunkIndex', chunkIndex.toString())
        formData.append('totalChunks', totalChunks.toString())
        formData.append('fileSize', file.size.toString())
        formData.append('clientId', getClientId())

        if (chunkIndex === 0) {
            const nonceBase64 = arrayBufferToBase64(fileNonce)
            formData.append('nonce', nonceBase64)
        }

        // Upload chunk
        await uploadChunk(token, formData, chunkIndex, relativePath)

        // Update progress
        completedChunks++
        updateFileProgress(fileItem, completedChunks, totalChunks)
    }

    // PHASE 1: Upload chunk 0 first (with nonce) - ensures nonce arrives before other chunks
    if (totalChunks > 0) {
        await prepareAndUploadChunk(0)
        console.timeEnd(`${relativePath} - chunk 0`);
    }

    // PHASE 2: Upload remaining chunks concurrently
    if (totalChunks > 1) {
        await runWithConcurrency(
            Array.from({ length: totalChunks - 1 }, (_, i) => i + 1),
            prepareAndUploadChunk,
            MAX_CONCURRENT_FILES
        )
    }

    console.timeEnd(`${relativePath} - total`);

    // Finalize (merge chunks)
    await finalizeFile(token, relativePath);

    const progressText = fileItem.querySelector('.progress-text')
    if (progressText) progressText.textContent = 'Upload complete!'
}

async function uploadChunk(token, formData, chunkIndex, relativePath) {
    const clientId = getClientId()
    const url = `/receive/${token}/chunk?clientId=${clientId}`

    return await retryWithExponentialBackoff(async () => {
        const response = await fetch(url, {
            method: 'POST',
            body: formData
        })

        if (!response.ok) {
            throw new Error(`HTTP ${response.status}`)
        }
        
        console.log(`✓ Chunk ${chunkIndex} of ${relativePath}`)
        
    }, 3, `chunk ${chunkIndex}`)
}

async function finalizeFile(token, relativePath) {
    const formData = new FormData();
    formData.append('relativePath', relativePath);
    
    const clientId = getClientId()
    const url = `/receive/${token}/finalize?clientId=${clientId}`
    const response = await fetch(url, {
        method: 'POST',
        body: formData
    });
    
    if (!response.ok) {
        throw new Error(`Failed to finalize ${relativePath}`);
    }
}



