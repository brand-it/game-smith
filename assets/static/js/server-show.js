// Auto-start / auto-restart toggle handlers for game server detail page.

function handleAutoToggle(toggleId, valueId, formId) {
  var checkbox = document.getElementById(toggleId);
  var hidden = document.getElementById(valueId);
  hidden.value = checkbox.checked ? 'true' : 'false';
  var form = document.getElementById(formId);
  if (form) form.requestSubmit();
}

function handleAutoStart() {
  handleAutoToggle('auto-start-toggle', 'auto-start-value', 'auto-start-form');
}

function handleAutoRestart() {
  handleAutoToggle('auto-restart-toggle', 'auto-restart-value', 'auto-restart-form');
}
