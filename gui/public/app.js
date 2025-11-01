let selectedProcess = null;
let refreshInterval = null;

// DOM elements
const processesList = document.getElementById('processes-list');
const logsContent = document.getElementById('logs-content');
const logsServiceName = document.getElementById('logs-service-name');
const refreshButton = document.getElementById('refresh');
const showAllCheckbox = document.getElementById('showAll');
const clearLogsButton = document.getElementById('clear-logs');

// Event listeners
refreshButton.addEventListener('click', () => {
  fetchProcesses();
  if (selectedProcess) {
    fetchLogs(selectedProcess);
  }
});

showAllCheckbox.addEventListener('change', fetchProcesses);

clearLogsButton.addEventListener('click', () => {
  logsContent.innerHTML = '';
});

// Fetch and display processes
async function fetchProcesses() {
  try {
    const showAll = showAllCheckbox.checked;
    const response = await fetch(`/api/processes?showAll=${showAll}`);
    const data = await response.json();

    if (data.message) {
      processesList.innerHTML = `<div class="empty-state">${data.message}</div>`;
      return;
    }

    if (data.processes.length === 0) {
      processesList.innerHTML = '<div class="empty-state">No active processes found</div>';
      return;
    }

    processesList.innerHTML = data.processes.map(process => `
      <div class="process-card ${selectedProcess === process.serviceName ? 'selected' : ''}"
           onclick="selectProcess('${process.serviceName}')">
        <div class="process-name">${process.serviceName}</div>
        <div class="process-info">
          <span class="process-info-label">Status:</span>
          <span class="process-info-value status-running">${process.status}</span>

          <span class="process-info-label">PID:</span>
          <span class="process-info-value">${process.pid}</span>

          <span class="process-info-label">Uptime:</span>
          <span class="process-info-value">${process.uptime}</span>

          <span class="process-info-label">Directory:</span>
          <span class="process-info-value">${process.workingDir}</span>
        </div>
      </div>
    `).join('');
  } catch (error) {
    console.error('Error fetching processes:', error);
    processesList.innerHTML = '<div class="empty-state">Error loading processes</div>';
  }
}

// Select a process and fetch its logs
function selectProcess(serviceName) {
  selectedProcess = serviceName;
  fetchProcesses(); // Refresh to update selected state
  fetchLogs(serviceName);
}

// Fetch and display logs for a service
async function fetchLogs(serviceName) {
  try {
    const response = await fetch(`/api/logs/${serviceName}`);
    const data = await response.json();

    logsServiceName.textContent = `Logs for ${data.serviceName}`;

    if (data.logs.length === 0) {
      logsContent.innerHTML = '<div class="empty-state">No logs available</div>';
      return;
    }

    logsContent.innerHTML = data.logs.map(log => {
      const date = new Date(log.timestamp * 1000);
      const timeStr = date.toLocaleTimeString();
      return `<div class="log-entry log-${log.type}">[${timeStr}] ${escapeHtml(log.content)}</div>`;
    }).join('');

    // Scroll to bottom
    logsContent.scrollTop = logsContent.scrollHeight;
  } catch (error) {
    console.error('Error fetching logs:', error);
    logsContent.innerHTML = '<div class="empty-state">Error loading logs</div>';
  }
}

// Utility function to escape HTML
function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

// Auto-refresh every 2 seconds
function startAutoRefresh() {
  refreshInterval = setInterval(() => {
    fetchProcesses();
    if (selectedProcess) {
      fetchLogs(selectedProcess);
    }
  }, 2000);
}

// Initial load
fetchProcesses();
startAutoRefresh();

// Make selectProcess available globally
window.selectProcess = selectProcess;
