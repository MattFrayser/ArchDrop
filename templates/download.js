// Detect browser capabilities once on page load
// Chrome bases allows for File System Access
// which vastly increases max download size
const browserCaps = detectBrowserCapabilities()
function detectBrowserCapabilities() {
    return {
        // File System Access API (Chrome, Edge, Opera, Brave)
        hasFileSystemAccess: 'showSaveFilePicker' in window,
        
        // Device memory in GB (Chrome-only, returns 2, 4, 8, etc.)
        deviceMemoryGB: navigator.deviceMemory || null,

        // iOS
        isIOS: /iPad|iPhone|iPod/.test(navigator.userAgent) || 
               (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1),
        
        // Estimated available memory in bytes
        estimatedMemory: navigator.deviceMemory 
            ? navigator.deviceMemory * 1024 * 1024 * 1024 
            : 4 * 1024 * 1024 * 1024, // Default 4GB
    }
}

//==============
// Global Cache
//=============
let cachedManifest = null
let cachedToken = null
let cachedClientId = null

// Download button
document.addEventListener('DOMContentLoaded', async () => {
    const downloadBtn = document.getElementById('downloadBtn');
    if (downloadBtn) {
        downloadBtn.addEventListener('click', startDownload);
    }

    // Load manifest and display files
    try {
        cachedToken = window.location.pathname.split('/').pop()
        cachedClientId = getClientId()
        const manifestResponse = await fetch(`/send/${cachedToken}/manifest?clientId=${cachedClientId}`)
        if (!manifestResponse.ok) {
            throw new Error(`Failed to fetch manifest: HTTP ${manifestResponse.status}`);
        }

        cachedManifest = await manifestResponse.json()
        displayFileList(cachedManifest.files)

    } catch (error) {
        console.error('Failed to load file list:', error)
    }
})

// List of files to download
function displayFileList(files) {
    const fileList = document.getElementById('fileList')
    if (!fileList || files.length === 0) return

    fileList.classList.add('show')

    files.forEach((file, index) => {
        const item = createFileItem(file, index, {
            initialProgressText: 'Ready to download',
            useSummaryWrapper: true
        })

        const progress = item.querySelector('.file-progress')
        if (progress) progress.classList.add('show')

        fileList.appendChild(item)
    })
}

//===========
// Logic
//===========
async function startDownload() {
    if (!cachedManifest || !cachedToken) {
        alert('File list not loaded. Please refresh the page.');
        return;
    }

    // Show progress bars
    const fileList = document.getElementById('fileList')
    const fileItems = fileList.querySelectorAll('.file-item')
    fileItems.forEach(item => {
        const progress = item.querySelector('.file-progress')
        if (progress) progress.classList.add('show')
    })

    try {
        // Get session key form url
        const { key } = await getCredentialsFromUrl()

        // download files concurrently
        await runWithConcurrency(
            cachedManifest.files.map((file, index) => ({ file, index, fileItem: fileItems[index] })),
            async ({ file, fileItem }) => {
                fileItem.classList.add('downloading')
                try {
                    await downloadFile(cachedToken, file, key, fileItem)
                    fileItem.classList.remove('downloading')
                    fileItem.classList.add('completed')
                } catch (error) {
                    fileItem.classList.remove('downloading')
                    fileItem.classList.add('error')
                    throw error
                }
            },
            MAX_CONCURRENT_FILES
        )

        // Completion
        await retryWithExponentialBackoff(async () => {
            const url = `/send/${cachedToken}/complete?clientId=${cachedClientId}`;
            const response = await fetch(url, { method: 'POST' });
            if (!response.ok) {
                throw new Error(`Completion handshake failed: ${response.status}`);
            }
        }, 5, 'Finalizing Transfer');

        const downloadBtn = document.getElementById('downloadBtn')
        downloadBtn.textContent = 'Download Complete!'

    } catch(error) {
        console.error(error)
        alert(`Download failed: ${error.message}`)
    }
}

async function downloadFile(token, fileEntry, key, fileItem) {
    const nonceBase = urlSafeBase64ToUint8Array(fileEntry.nonce)
    const totalChunks = Math.ceil(fileEntry.size / CHUNK_SIZE)

    if (browserCaps.hasFileSystemAccess && fileEntry.size > FILE_SYSTEM_API_THRESHOLD) {
        console.log(`Using File System API for ${fileEntry.name} (${formatFileSize(fileEntry.size)})`)
        await downloadViaFileSystemAPI(token, fileEntry, key, nonceBase, totalChunks, fileItem)
    } else {
        // Check if file might be too large for available memory
        if (fileEntry.size > browserCaps.estimatedMemory * 0.5) {
            await showMemoryWarning(fileEntry)
        }

        console.log(`Using in-memory download for ${fileEntry.name}`)
        await downloadViaBlob(token, fileEntry, key, nonceBase, totalChunks, fileItem)
    }

    // TODO: Re-implement hash verification with proper stream piping
}

// Sliding Window Stram 
// for iOS, Safari support, maybe other browsers too
function getDecryptedChunkStream(token, fileEntry, key, totalChunks, fileItem) {
    const nonceBase = urlSafeBase64ToUint8Array(fileEntry.nonce)

    let nextFetch = 0
    let nextYield = 0

    // Buffer for out of order
    // Buffer will never have more than max (6) chunks (max 6mb), fine for memory 
    const chunkBuffer = new Map()
    let activeFetches = 0

    // Even thought Files must arrive in correct order. Can still use concurrecy
    // nextFext sends out up to max limit of chunkReq or room in buffer
    // nextYield is holds results of those fetches. 
    // Chunks are taken out of next yeild in order.
    let pendingPullResolver = null;

    return new ReadableStream({
        async pull(controller) {
            // Helper to push data if available
            const tryPush = () => {
                // If we have the next needed chunk in buffer, yield it immediately
                while (chunkBuffer.has(nextYield)) {
                    const data = chunkBuffer.get(nextYield);
                    chunkBuffer.delete(nextYield);
                    controller.enqueue(data);
                    
                    nextYield++;
                    updateFileProgress(fileItem, nextYield, totalChunks);
                    
                    if (nextYield >= totalChunks) {
                        return true; // Done
                    }
                }
                return false;
            };

            // Try to yield what we already have
            if (tryPush()) {
                controller.close();
                return;
            }

            // Refill the window of active fetches
            while (activeFetches < MAX_CONCURRENT_CHUNKS && nextFetch < totalChunks) {
                const chunkIndex = nextFetch++;
                activeFetches++;

                // Trigger fetch (don't await here, let it run in background)
                fetchAndDecrypt(token, fileEntry.index, chunkIndex, key, nonceBase)
                    .then(decrypted => {
                        chunkBuffer.set(chunkIndex, new Uint8Array(decrypted));
                        activeFetches--;

                        // If we were waiting for this specific chunk, wake up the pull loop
                        if (chunkIndex === nextYield && pendingPullResolver) {
                            pendingPullResolver();
                            pendingPullResolver = null;
                        }
                    })
                    .catch(err => {
                        controller.error(err);
                    });
            }
            
            // Stalled (buffer doesn't have the *next* ordered chunk)
            if (!chunkBuffer.has(nextYield) && activeFetches > 0) {
                await new Promise(resolve => pendingPullResolver = resolve);
                
                // When we wake up, try to push again
                if (tryPush()) {
                    controller.close();
                }
            }
        }
    });
}

/*  CONCURRENT APPROACH - Had issues with safari, but want to keep for future 
                          Could have different brower features
function getDecryptedChunkStream(token, fileEntry, key, totalChunks, fileItem) {
    const nonceBase = urlSafeBase64ToUint8Array(fileEntry.nonce)
    let completedChunks = 0

    return new ReadableStream({
        async start(controller) {
            // Array to store chunks in order - PROBLEM: buffers entire file in memory!
            const orderedChunks = new Array(totalChunks)

            // Use concurrency helper to download chunks in parallel
            await runWithConcurrency(
                Array.from({ length: totalChunks }, (_, i) => i),
                async (chunkIndex) => {
                    try {
                        const encrypted = await downloadChunk(token, fileEntry.index, chunkIndex)
                        const nonce = generateNonce(nonceBase, chunkIndex)

                        const decrypted = await window.crypto.subtle.decrypt(
                            { name: 'AES-GCM', iv: nonce },
                            key,
                            encrypted
                        )

                        // Store chunk in correct position instead of enqueuing immediately
                        orderedChunks[chunkIndex] = new Uint8Array(decrypted)

                        completedChunks++
                        updateFileProgress(fileItem, completedChunks, totalChunks)
                    } catch (e) {
                        console.error(`Error processing chunk ${chunkIndex}:`, e)
                        // Error handling: Abort the stream on chunk failure
                        controller.error(e)
                        throw e
                    }
                },
                MAX_CONCURRENT_FILES
            )

            // Enqueue all chunks in correct order - PROBLEM: only starts after ALL chunks buffered
            for (const chunk of orderedChunks) {
                controller.enqueue(chunk)
            }

            controller.close()
        },
    })
}
*/


// Transform Stream for memory-efficient hash calculation
class HashingTransformStream {
    constructor() {
        this.collectedChunks = []
        this.hashPromise = new Promise(resolve => this.resolveHash = resolve)
        
        // This is the standard TransformStream API implementation
        this.transformStream = new TransformStream({
            transform: (chunk, controller) => {
                // Collect chunks into a buffer for final hashing
                this.collectedChunks.push(chunk)
                // Pass the chunk down the pipe immediately for writing
                controller.enqueue(chunk) 
            },
            flush: () => {
                // The stream is done; now, compute the hash
                this._computeHash() 
            }
        })
    }

    get writable() {
        return this.transformStream.writable
    }

    get readable() {
        return this.transformStream.readable
    }
    
    // Method to be called by downloadFile to get the final hash
    async getComputedHash() {
        return this.hashPromise
    }
    
    async _computeHash() {
        // Create one Blob from all collected chunks (only Copy 2 is made)
        const fullFileBlob = new Blob(this.collectedChunks)
        
        // Read the Blob into an ArrayBuffer for crypto.subtle.digest()
        const arrayBuffer = await fullFileBlob.arrayBuffer()
        
        // Compute local hash
        const hashBuffer = await window.crypto.subtle.digest('SHA-256', arrayBuffer)
        const hashArray = Array.from(new Uint8Array(hashBuffer))
        const computedHash = hashArray
            .map(b => b.toString(16).padStart(2,'0'))
            .join('')
            
        this.resolveHash(computedHash)
        // Free up memory from collected chunks after hashing
        this.collectedChunks = [] 
    }
}

async function downloadViaFileSystemAPI(token, fileEntry, key, nonceBase, totalChunks, fileItem) {
    // Prompt user to save file
    const fileHandle = await window.showSaveFilePicker({
        suggestedName: fileEntry.name,
    })

    const writable = await fileHandle.createWritable()

    try {
        // Create the decrypted stream
        const stream = getDecryptedChunkStream(token, fileEntry, key, totalChunks, fileItem)

        // Pipe the verifiable stream directly to the disk writable
        await stream.pipeTo(writable)
        
        // Update UI
        const progressText = fileEntry.fileItem.querySelector('.progress-text')
        if (progressText) progressText.textContent = 'Download complete!'
        
    } catch (error) {
        await writable.abort()
        throw error
    }
}

// In-memory blob path (Firefox/Safari/small files)
async function downloadViaBlob(token, fileEntry, key, nonceBase, totalChunks, fileItem) {
    // Create the decrypted stream
    const stream = getDecryptedChunkStream(token, fileEntry, key, totalChunks, fileItem)

    // Collect the stream into a Response
    const response = new Response(stream)
    
    // Create a Blob from the Response stream
    const blob = await response.blob()

    // Trigger download (standard browser action)
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = fileEntry.name
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    URL.revokeObjectURL(url)
}

async function showMemoryWarning(fileEntry) {
    const fileSize = formatFileSize(fileEntry.size)
    const availableMem = formatFileSize(browserCaps.estimatedMemory)
    
    const message = `
        Warning: This file (${fileSize}) is very large and may use significant memory.

        Available memory: ~${availableMem}
        Your browser: ${browserCaps.hasFileSystemAccess ? 'Chrome/Edge' : 'Firefox/Safari'}

        ${browserCaps.hasFileSystemAccess ? '' : 'Recommendation: Use Chrome or Edge for files over 200MB for better memory efficiency.\n\n'}
        Continue download?
    `
    
    if (!confirm(message)) {
        throw new Error('Download cancelled by user')
    }
}

async function fetchAndDecrypt(token, fileIndex, chunkIndex, key, nonceBase) {
    const encrypted = await downloadChunk(token, fileIndex, chunkIndex);
    const nonce = generateNonce(nonceBase, chunkIndex);
    
    return await window.crypto.subtle.decrypt(
        { name: 'AES-GCM', iv: nonce },
        key,
        encrypted
    );
}

async function verifyHash(blob, fileEntry, token) {
    // Compute local hash
    const arrayBuffer = await blob.arrayBuffer()
    const hashBuffer = await crypto.subtle.digest('SHA-256', arrayBuffer)
    const hashArray = Array.from(new Uint8Array(hashBuffer))
    const computedHash = hashArray
        .map(b => b.toString(16).padStart(2,'0'))
        .join('')

    // Request hash from server - use cached client ID
    const clientId = cachedClientId
    const response = await fetch(`/send/${token}/${fileEntry.index}/hash?clientId=${clientId}`)
    if (!response.ok) {
        console.warn(`Could not verify ${fileEntry.name}: ${response.status}`)
        return // Skip if hash unavailable
    }

    const { sha256 } = await response.json()

    if (computedHash !== sha256) {
        throw new Error(`File integrity check failed! Expected ${sha256}, got ${computedHash}`)
    }
}

async function downloadChunk(token, fileIndex, chunkIndex, maxRetries = 3) {
    // Use cached client ID to ensure consistency across all requests
    const clientId = cachedClientId

    return await retryWithExponentialBackoff(async () => {
        const response = await fetch(`/send/${token}/${fileIndex}/chunk/${chunkIndex}?clientId=${clientId}`)
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}`)
        }
        return await response.arrayBuffer()
    }, maxRetries, `chunk ${chunkIndex}`)
}


