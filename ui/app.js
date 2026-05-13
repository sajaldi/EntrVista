const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;
const { appWindow } = window.__TAURI__.window;

// Window controls
document.getElementById('minimize-btn').onclick = () => appWindow.minimize();
document.getElementById('maximize-btn').onclick = () => appWindow.toggleMaximize();
document.getElementById('close-btn').onclick = () => appWindow.close();

// Minimize on double click
document.ondblclick = () => appWindow.minimize();

// Clear session / New interview
document.getElementById('clear-session-btn').onclick = async () => {
  try {
    await invoke('clear_session');
    // Clear transcript UI
    const feed = document.getElementById('transcript-feed');
    if (feed) {
      feed.innerHTML = '';
    }
    // Clear AI view
    const aiView = document.getElementById('ai-view');
    if (aiView) {
      aiView.style.display = 'none';
    }
    console.log("🧹 Session cleared");
  } catch (err) {
    console.error("Failed to clear session:", err);
  }
};
 
// Language switching logic
const btnEn = document.getElementById('lang-en-btn');
const btnEs = document.getElementById('lang-es-btn');

async function setLanguage(lang) {
  try {
    await invoke('set_language', { lang });
    if (lang === 'en') {
      btnEn.classList.add('active');
      btnEs.classList.remove('active');
    } else {
      btnEs.classList.add('active');
      btnEn.classList.remove('active');
    }
  } catch (err) {
    console.error("Failed to set language:", err);
  }
}

btnEn.onclick = () => setLanguage('en');
btnEs.onclick = () => setLanguage('es');

// Modal logic
const modal = document.getElementById('answer-modal');
const modalContent = document.getElementById('modal-content');
const aiView = document.getElementById('ai-view');
const aiContent = document.getElementById('ai-content');
const teleprompter = document.getElementById('teleprompter');
let lastAiResponseText = ""; // Track the raw text to save it if needed

document.getElementById('modal-close').onclick = () => modal.style.display = 'none';
document.getElementById('close-ai-btn').onclick = () => aiView.style.display = 'none';

modal.onclick = (e) => { if (e.target === modal) modal.style.display = 'none'; };
aiView.onclick = (e) => { if (e.target === aiView) aiView.style.display = 'none'; };

function showAiResponse(text) {
  lastAiResponseText = text; // Store for "Save as Fact"
  aiContent.innerHTML = formatStarResponse(text);
  aiView.style.display = 'flex';
  aiView.scrollTop = 0;
}

document.getElementById('save-as-fact-btn').onclick = async (e) => {
  e.stopPropagation(); // Don't close the view
  if (!lastAiResponseText) return;

  const title = prompt("Enter a title for this Fact:", "Summary of AI response");
  if (!title) return;

  try {
    await invoke('save_predefined_item', { 
      itemType: 'fact', 
      title: title, 
      content: lastAiResponseText 
    });
    alert("✅ Response saved as Fact!");
    await initPredefined(); // Refresh local cache
  } catch (err) {
    alert("❌ Failed to save fact: " + err);
  }
};

function openModal(text, isLoading = false) {
  if (!isLoading && (!text || text.trim() === "")) return;
  if (isLoading) {
    modalContent.innerHTML = '<div class="modal-loading"><span></span><span></span><span></span></div>';
  } else {
    const formatted = text
      .replace(/\*\*Situation\*\*/g, '<strong class="star-tag">📍 Situation</strong>')
      .replace(/\*\*Task\*\*/g, '<strong class="star-tag">🎯 Task</strong>')
      .replace(/\*\*Action\*\*/g, '<strong class="star-tag">⚡ Action</strong>')
      .replace(/\*\*Result\*\*/g, '<strong class="star-tag">✅ Result</strong>')
      .replace(/\n/g, '<br>');
    modalContent.innerHTML = formatted;
  }
  modal.style.display = 'flex';
}

// Live interim transcript indicator
let interimBubble = null;

listen('new-interim', (event) => {
  const feed = document.getElementById('transcript-feed');
  const text = event.payload;

  if (!interimBubble) {
    interimBubble = document.createElement('div');
    interimBubble.className = 'bubble interim-transcript';
    interimBubble.style.opacity = '0.5';
    interimBubble.style.fontStyle = 'italic';
    feed.prepend(interimBubble);
  }
  interimBubble.innerText = '🎙️ ' + text;
});

// Listen for live transcription (final)
listen('new-transcript', (event) => {
  // Remove the interim bubble since we now have the final
  if (interimBubble) {
    interimBubble.remove();
    interimBubble = null;
  }
  addMessage(event.payload, 'user-transcript');
});

// Listen for direct AI answers
listen('new-answer', (event) => {
  showAiResponse(event.payload);
});

// Listen for AI options (the new flow)
listen('new-options', (event) => {
  const { question, options } = event.payload;
  showOptions(question, options);
});

// Listen for context/model switches
listen('context-switch', (event) => {
  document.getElementById('context-badge').innerText = event.payload;
});

function formatStarResponse(text) {
  return text
    .replace(/\*\*Situation\*\*/g, '📍 Situation:')
    .replace(/\*\*Task\*\*/g, '🎯 Task:')
    .replace(/\*\*Action\*\*/g, '⚡ Action:')
    .replace(/\*\*Result\*\*/g, '✅ Result:')
    .replace(/\n\n/g, '<br><br>')
    .replace(/\n/g, '<br>');
}

// Transcript selection state
let selectedBubbles = [];
const multiActions = document.getElementById('multi-actions');
const multiAnalyzeBtn = document.getElementById('multi-analyze-btn');
const multiCopyBtn = document.getElementById('multi-copy-btn');

function updateMultiAnalyzeButton() {
  if (selectedBubbles.length > 0) {
    multiActions.style.display = 'flex';
    multiAnalyzeBtn.innerText = `Analyze (${selectedBubbles.length})`;
  } else {
    multiActions.style.display = 'none';
  }
}

function getSelectedText() {
  return selectedBubbles
    .map(b => b.innerText.replace('Z', '').trim())
    .reverse() // feed is prepend, so reverse to get chronological
    .join('\n');
}

multiCopyBtn.onclick = async () => {
  if (selectedBubbles.length === 0) return;
  
  const textToCopy = getSelectedText();
  try {
    await navigator.clipboard.writeText(textToCopy);
    const originalText = multiCopyBtn.innerText;
    multiCopyBtn.innerText = "Copied! ✅";
    setTimeout(() => multiCopyBtn.innerText = originalText, 2000);
  } catch (err) {
    console.error("Clipboard failed:", err);
  }
};

multiAnalyzeBtn.onclick = async () => {
  if (selectedBubbles.length === 0) return;
  
  const combinedText = getSelectedText().replace(/\n/g, ' ');

  console.log("🚀 Multi-analyze request:", combinedText);
  
  // Clear selection after getting text
  selectedBubbles.forEach(b => b.classList.remove('selected'));
  selectedBubbles = [];
  updateMultiAnalyzeButton();

  showAiResponse("⌛ AI is analyzing combined question...");
  try {
    const answer = await invoke('ask_zai_specifically', { text: combinedText });
    showAiResponse(answer);
  } catch (err) {
    showAiResponse(`❌ Error: ${err}`);
  }
};

/**
 * Shows the options panel for a detected question.
 */
function showOptions(question, options) {
  const feed = document.getElementById('transcript-feed');

  if (!options || options.length === 0) {
    addMessage('⚠️ Could not generate options.', 'assistant-answer');
    return;
  }

  const card = document.createElement('div');
  card.className = 'options-card';

  const header = document.createElement('div');
  header.className = 'options-header';
  header.innerHTML = `<span class="options-icon">💡</span> Select your answer angle:`;
  card.appendChild(header);

  const list = document.createElement('div');
  list.className = 'options-list';

  options.forEach((opt, i) => {
    const btn = document.createElement('button');
    btn.className = 'option-btn';
    btn.innerHTML = `<span class="opt-num">${i + 1}</span>${opt}`;
    btn.onclick = async () => {
      showAiResponse("⌛ Preparing your answer...");
      try {
        const prompt = `Answer this interview question: "${question}"\n\nFocus specifically on this angle: ${opt}`;
        const answer = await invoke('ask_zai_specifically', { text: prompt });
        showAiResponse(answer);
      } catch (err) {
        showAiResponse(`❌ Error: ${err}`);
      }
    };
    list.appendChild(btn);
  });

  card.appendChild(list);
  feed.prepend(card);
}

function addMessage(text, type) {
  const feed = document.getElementById('transcript-feed');

  if (type === 'assistant-answer') {
    showAiResponse(text);
    return;
  }

  const bubble = document.createElement('div');
  bubble.className = `bubble ${type}`;
  bubble.innerText = text;

  // Selection logic for transcripts
  if (type === 'user-transcript') {
    bubble.style.cursor = 'pointer';
    bubble.onclick = () => {
      if (bubble.classList.contains('selected')) {
        bubble.classList.remove('selected');
        selectedBubbles = selectedBubbles.filter(b => b !== bubble);
      } else {
        bubble.classList.add('selected');
        selectedBubbles.push(bubble);
      }
      updateMultiAnalyzeButton();
    };

    const btn = document.createElement('button');
    btn.className = 'analyze-btn';
    btn.innerText = 'Z';
    btn.onclick = async (e) => {
      e.stopPropagation();
      
      let textToAnalyze = text;
      
      // If there's a selection, use it instead of just this bubble
      if (selectedBubbles.length > 0) {
        textToAnalyze = getSelectedText().replace(/\n/g, ' ');
          
        // Clear selection
        selectedBubbles.forEach(b => b.classList.remove('selected'));
        selectedBubbles = [];
        updateMultiAnalyzeButton();
      }

      bubble.classList.add('loading');
      showAiResponse("⌛ AI is generating STAR answer...");
      try {
        const answer = await invoke('ask_zai_specifically', { text: textToAnalyze });
        showAiResponse(answer);
      } catch (err) {
        showAiResponse(`❌ Error: ${err}`);
      } finally {
        bubble.classList.remove('loading');
      }
    };
    bubble.appendChild(btn);
  }

  feed.prepend(bubble);
}

// Respuestas persistentes en SQLite
let predefinedData = { responses: [], lifesavers: [], facts: [] };
let currentTab = 'response'; // 'response' or 'lifesaver' or 'fact'
let editingId = null;

const settingsModal = document.getElementById('settings-modal');
const settingsList = document.getElementById('settings-list');
const quickQuestions = document.getElementById('quick-questions');
const tabResponses = document.getElementById('tab-responses');
const tabLifesavers = document.getElementById('tab-lifesavers');
const tabFacts = document.getElementById('tab-facts');
const formTitle = document.getElementById('form-title');
const itemTitleInput = document.getElementById('item-title');
const itemContentInput = document.getElementById('item-content');
const saveItemBtn = document.getElementById('save-item-btn');
const cancelEditBtn = document.getElementById('cancel-edit-btn');

async function initPredefined() {
  try {
    const data = await invoke('get_all_predefined');
    predefinedData.responses = data.responses;
    predefinedData.lifesavers = data.lifesavers;
    predefinedData.facts = data.facts;
    renderQuickQuestions();
    console.log("✅ Predefined data loaded from SQLite");
  } catch (err) {
    console.error("❌ Failed to load predefined data:", err);
  }
}

function renderQuickQuestions() {
  quickQuestions.innerHTML = '';
  
  // Render Lifesavers first (they have a different style)
  predefinedData.lifesavers.forEach(item => {
    const btn = document.createElement('button');
    btn.className = 'q-chip lifesaver';
    btn.style.webkitAppRegion = 'no-drag';
    btn.innerText = item.title;
    btn.onclick = () => askPredefined(item.title);
    quickQuestions.appendChild(btn);
  });

  // Render Responses
  predefinedData.responses.forEach(item => {
    const btn = document.createElement('button');
    btn.className = 'q-chip';
    btn.style.webkitAppRegion = 'no-drag';
    btn.innerText = item.title;
    btn.onclick = () => askPredefined(item.title);
    quickQuestions.appendChild(btn);
  });
}

function renderSettingsList() {
  settingsList.innerHTML = '';
  const items = currentTab === 'response' ? predefinedData.responses : 
               (currentTab === 'lifesaver' ? predefinedData.lifesavers : predefinedData.facts);

  items.forEach(item => {
    const div = document.createElement('div');
    div.className = 'settings-item';
    div.innerHTML = `
      <div class="item-info">
        <h4>${item.title}</h4>
        <p>${item.content}</p>
      </div>
      <div class="item-actions">
        <button class="action-btn edit-btn" title="Edit">✏️</button>
        <button class="action-btn delete-btn" title="Delete">🗑️</button>
      </div>
    `;

    div.querySelector('.edit-btn').onclick = () => startEdit(item);
    div.querySelector('.delete-btn').onclick = () => deleteItem(item.id);

    settingsList.appendChild(div);
  });
}

function startEdit(item) {
  editingId = item.id;
  itemTitleInput.value = item.title;
  itemContentInput.value = item.content;
  formTitle.innerText = `Edit ${currentTab === 'response' ? 'Response' : (currentTab === 'lifesaver' ? 'Lifesaver' : 'Fact')}`;
  cancelEditBtn.style.display = 'block';
  itemTitleInput.focus();
}

function resetForm() {
  editingId = null;
  itemTitleInput.value = '';
  itemContentInput.value = '';
  formTitle.innerText = `Add New ${currentTab === 'response' ? 'Response' : (currentTab === 'lifesaver' ? 'Lifesaver' : 'Fact')}`;
  cancelEditBtn.style.display = 'none';
}

async function deleteItem(id) {
  if (!confirm('Are you sure you want to delete this item?')) return;
  try {
    await invoke('delete_predefined_item', { itemType: currentTab, id });
    await initPredefined();
    renderSettingsList();
  } catch (err) {
    alert("Error deleting item: " + err);
  }
}

saveItemBtn.onclick = async () => {
  const title = itemTitleInput.value.trim();
  const content = itemContentInput.value.trim();

  if (!title || !content) {
    alert("Title and Content are required");
    return;
  }

  try {
    await invoke('save_predefined_item', { itemType: currentTab, title, content });
    resetForm();
    await initPredefined();
    renderSettingsList();
  } catch (err) {
    alert("Error saving item: " + err);
  }
};

cancelEditBtn.onclick = resetForm;

tabResponses.onclick = () => {
  currentTab = 'response';
  tabResponses.classList.add('active');
  tabLifesavers.classList.remove('active');
  tabFacts.classList.remove('active');
  resetForm();
  renderSettingsList();
};

tabLifesavers.onclick = () => {
  currentTab = 'lifesaver';
  tabLifesavers.classList.add('active');
  tabResponses.classList.remove('active');
  tabFacts.classList.remove('active');
  resetForm();
  renderSettingsList();
};

tabFacts.onclick = () => {
  currentTab = 'fact';
  tabFacts.classList.add('active');
  tabResponses.classList.remove('active');
  tabLifesavers.classList.remove('active');
  resetForm();
  renderSettingsList();
};

document.getElementById('settings-btn').onclick = () => {
  settingsModal.style.display = 'flex';
  
  // Reset position
  if (typeof xOffset !== 'undefined') {
    xOffset = 0; yOffset = 0;
    document.querySelector('#settings-modal .modal-box').style.transform = 'translate(0px, 0px)';
  }
  
  renderSettingsList();
};

document.getElementById('settings-close').onclick = () => {
  settingsModal.style.display = 'none';
};

async function askPredefined(question) {
  console.log("Asking predefined question:", question);
  
  // Look in both responses and lifesavers
  const allItems = [...predefinedData.responses, ...predefinedData.lifesavers];
  const item = allItems.find(i => i.title === question);

  if (item) {
    showAiResponse(item.content);
  } else {
    showAiResponse("⌛ Generating perfect answer for: " + question);
    try {
      const answer = await invoke('ask_zai_specifically', { text: question });
      showAiResponse(answer);
    } catch (err) {
      showAiResponse(`❌ Error: ${err}`);
    }
  }
}

// Initialize on load
initPredefined();


// Exponer a la ventana global para que el HTML la vea siempre
window.askPredefined = askPredefined;

const gestureDebug = document.getElementById('gesture-debug');
const videoElement = document.getElementById('webcam-video');
const webcamContainer = document.getElementById('webcam-container');
const handPointer = document.getElementById('hand-pointer');
let activeStream = null;
let availableCameras = [];
let currentCameraIndex = 1; // FIJAR A CÁMARA 2
let hands = null;
const canvasElement = document.getElementById('webcam-canvas');
const canvasCtx = canvasElement.getContext('2d');

let isCameraActive = true;
const cameraToggleBtn = document.getElementById('camera-toggle-btn');
cameraToggleBtn.classList.add('active');

cameraToggleBtn.onclick = () => {
  if (isCameraActive) {
    stopCamera();
  } else {
    startCamera(currentCameraIndex);
    isCameraActive = true;
    cameraToggleBtn.classList.add('active');
  }
};

function stopCamera() {
  if (activeStream) {
    activeStream.getTracks().forEach(track => track.stop());
    activeStream = null;
  }
  videoElement.srcObject = null;
  isCameraActive = false;
  cameraToggleBtn.classList.remove('active');
  handPointer.style.display = 'none'; // Hide pointer
  gestureDebug.innerText = "Camera Off";
  webcamContainer.style.opacity = "0.5"; // Dim container
}

async function startCamera(index) {
  if (availableCameras.length === 0) return;

  // Ajustar el índice si el deseado no existe
  if (index >= availableCameras.length) index = 0;
  currentCameraIndex = index;

  if (activeStream) {
    activeStream.getTracks().forEach(track => track.stop());
  }

  const deviceId = availableCameras[index]?.deviceId;
  try {
    gestureDebug.innerText = "🔌 Connecting...";
    activeStream = await navigator.mediaDevices.getUserMedia({
      video: { deviceId: deviceId ? { exact: deviceId } : undefined }
    });
    videoElement.srcObject = activeStream;

    videoElement.onloadedmetadata = () => {
      videoElement.play();
      canvasElement.width = videoElement.videoWidth;
      canvasElement.height = videoElement.videoHeight;
      gestureDebug.innerText = `📷 Cam ${index + 1} ACTIVE`;
      webcamContainer.style.display = 'block';
      webcamContainer.style.opacity = "1";
      runDetection();
    };
  } catch (err) {
    gestureDebug.innerText = "❌ Access Denied";
  }
}

async function runDetection() {
  if (!activeStream || !hands || videoElement.paused) {
    requestAnimationFrame(runDetection);
    return;
  }

  // Draw to canvas then send to AI
  canvasCtx.drawImage(videoElement, 0, 0, canvasElement.width, canvasElement.height);
  try {
    await hands.send({ image: canvasElement });
  } catch (e) {
    console.error(e);
  }

  requestAnimationFrame(runDetection);
}

async function initGestures() {
  if (typeof Hands === 'undefined') {
    gestureDebug.innerText = "❌ Libs fail";
    return;
  }

  gestureDebug.innerText = "🧠 Loading AI...";
  const devices = await navigator.mediaDevices.enumerateDevices();
  availableCameras = devices.filter(device => device.kind === 'videoinput');

  hands = new Hands({
    locateFile: (file) => `https://cdn.jsdelivr.net/npm/@mediapipe/hands/${file}`
  });

  hands.setOptions({
    maxNumHands: 1,
    modelComplexity: 0,
    minDetectionConfidence: 0.3,
    minTrackingConfidence: 0.3
  });

  const teleprompter = document.getElementById('teleprompter');
  const modalBox = document.querySelector('.modal-box');
  const modalOverlay = document.getElementById('answer-modal');

  hands.onResults((results) => {
    if (results.multiHandLandmarks && results.multiHandLandmarks.length > 0) {
      gestureDebug.innerText = "🖐️ DETECTED";
      gestureDebug.style.color = "#00ffcc";
      webcamContainer.style.borderColor = "#00ffcc";
      handPointer.style.display = "block";

      const indexFinger = results.multiHandLandmarks[0][8];
      handPointer.style.left = `${(1 - indexFinger.x) * 100}%`;
      handPointer.style.top = `${indexFinger.y * 100}%`;

      // --- DETECTAR TARGET ---
      const isModalOpen = modalOverlay && modalOverlay.style.display !== 'none';
      const isAiOpen = aiView && aiView.style.display !== 'none';

      let target = teleprompter;
      if (isModalOpen) target = modalBox;
      else if (isAiOpen) target = aiView;

      if (!target) return;

      // --- LÓGICA DE SCROLL ULTRA-SENSIBLE (MODIFICADA) ---
      const y = indexFinger.y;
      if (y < 0.48) { // Zona muerta mucho más estrecha
        const speed = Math.pow((0.48 - y) * 25, 1.6); // Multiplicador subido de 15 a 25
        target.scrollBy(0, -speed);
        handPointer.style.background = "#ff4b4b";
      } else if (y > 0.52) { // Zona muerta mucho más estrecha
        const speed = Math.pow((y - 0.52) * 25, 1.6); // Exponente y multiplicador más agresivos
        target.scrollBy(0, speed);
        handPointer.style.background = "#ff4b4b";
      } else {
        handPointer.style.background = "#ffffff";
      }
    } else {
      gestureDebug.innerText = "⌛ Ready. Show hand";
      gestureDebug.style.color = "white";
      webcamContainer.style.borderColor = "rgba(255,255,255,0.3)";
      handPointer.style.display = "none";
    }
  });

  startCamera(currentCameraIndex);
}

initGestures();

// --- Drag Logic for Settings Modal ---
const dragHandle = document.getElementById('settings-drag-handle');
const modalBox = document.querySelector('#settings-modal .modal-box');
let isDragging = false;
let currentX;
let currentY;
let initialX;
let initialY;
let xOffset = 0;
let yOffset = 0;

dragHandle.addEventListener('mousedown', dragStart);
document.addEventListener('mouseup', dragEnd);
document.addEventListener('mousemove', drag);

function dragStart(e) {
  initialX = e.clientX - xOffset;
  initialY = e.clientY - yOffset;
  if (e.target === dragHandle) {
    isDragging = true;
  }
}

function dragEnd(e) {
  initialX = currentX;
  initialY = currentY;
  isDragging = false;
}

function drag(e) {
  if (isDragging) {
    e.preventDefault();
    currentX = e.clientX - initialX;
    currentY = e.clientY - initialY;
    xOffset = currentX;
    yOffset = currentY;
    modalBox.style.transform = `translate(${currentX}px, ${currentY}px)`;
  }
}
