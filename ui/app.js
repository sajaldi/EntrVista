const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;
const { appWindow } = window.__TAURI__.window;

// Window controls
document.getElementById('minimize-btn').onclick = () => appWindow.minimize();
document.getElementById('maximize-btn').onclick = () => appWindow.toggleMaximize();
document.getElementById('close-btn').onclick = () => appWindow.close();

// Modal logic
const modal = document.getElementById('answer-modal');
const modalContent = document.getElementById('modal-content');
document.getElementById('modal-close').onclick = () => modal.style.display = 'none';
modal.onclick = (e) => { if (e.target === modal) modal.style.display = 'none'; };

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
  addMessage(event.payload, 'assistant-answer');
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

/**
 * Shows the options panel for a detected question.
 * Each option is clickable and triggers a full STAR answer in the modal.
 */
function showOptions(question, options) {
  const feed = document.getElementById('transcript-feed');

  // Don't render empty card
  if (!options || options.length === 0) {
    addMessage('⚠️ Could not generate options. Check console for API error.', 'assistant-answer');
    return;
  }

  const card = document.createElement('div');
  card.className = 'options-card';

  const header = document.createElement('div');
  header.className = 'options-header';
  header.innerHTML = `<span class="options-icon">💡</span> Choose an angle to answer:`;
  card.appendChild(header);

  const list = document.createElement('div');
  list.className = 'options-list';

  options.forEach((opt, i) => {
    const btn = document.createElement('button');
    btn.className = 'option-btn';
    btn.innerHTML = `<span class="opt-num">${i + 1}</span>${opt}`;
    btn.onclick = async () => {
      // Mark selected
      list.querySelectorAll('.option-btn').forEach(b => b.classList.remove('selected'));
      btn.classList.add('selected');

      // Open modal with loading state
      openModal('', true);

      try {
        const prompt = `Answer this interview question: "${question}"\n\nFocus specifically on this angle: ${opt}`;
        const answer = await invoke('ask_zai_specifically', { text: prompt });
        openModal(answer);
      } catch (err) {
        openModal(`Error: ${err}`);
      }
    };
    list.appendChild(btn);
  });

  card.appendChild(list);
  feed.prepend(card);
}

function addMessage(text, type) {
  const feed = document.getElementById('transcript-feed');
  const bubble = document.createElement('div');
  bubble.className = `bubble ${type}`;

  const content = document.createElement('div');
  content.className = 'content';
  content.innerText = text;
  bubble.appendChild(content);

  // Click to expand AI answers
  if (type === 'assistant-answer') {
    bubble.style.cursor = 'pointer';
    bubble.title = 'Click to expand';
    bubble.onclick = () => openModal(text);
  }

  // "Z" manual button for transcripts
  if (type === 'user-transcript') {
    const btn = document.createElement('button');
    btn.className = 'analyze-btn';
    btn.innerText = 'Z';
    btn.onclick = async (e) => {
      e.stopPropagation();
      bubble.classList.add('loading');
      openModal('Connecting to AI...', true);
      try {
        console.log("🚀 Invoking ask_zai_specifically for:", text);
        const answer = await invoke('ask_zai_specifically', { text });
        if (!answer || answer.trim() === "") {
          throw new Error("AI returned an empty response. Check if model name is correct.");
        }
        openModal(answer);
      } catch (err) {
        console.error("❌ AI Error:", err);
        openModal(`Error reaching AI: ${err}. \n\nCheck terminal for more details.`);
      } finally {
        bubble.classList.remove('loading');
      }
    };
    bubble.appendChild(btn);
  }

  feed.prepend(bubble);
}
