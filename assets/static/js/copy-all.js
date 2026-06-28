// Copy commands for each manual install section.

var COPY_BUTTON_CLASS = "copy-section-btn";
var COPY_LABEL_CLASS = "copy-section-label";
var CODE_COMMANDS = "code.font-mono";

(function () {
  var buttons = document.querySelectorAll("." + COPY_BUTTON_CLASS);
  buttons.forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.preventDefault();
      e.stopPropagation();

      var summary = btn.closest("summary");
      if (!summary) return;

      var details = summary.closest("details");
      if (!details) return;

      var codes = details.querySelectorAll(CODE_COMMANDS);
      var commands = [];
      codes.forEach(function (el) {
        var text = el.textContent.trim();
        if (text) commands.push(text);
      });

      var text = commands.join("\n\n");
      if (!text) return;

      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(function () { flash(btn); });
      } else {
        var ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
        flash(btn);
      }
    });
  });

  function flash(btn) {
    var label = btn.querySelector("." + COPY_LABEL_CLASS);
    label.textContent = "Copied!";
    btn.classList.remove("bg-green-600", "hover:bg-green-700");
    btn.classList.add("bg-green-700");
    setTimeout(function () {
      label.textContent = "Copy";
      btn.classList.add("bg-green-600", "hover:bg-green-700");
      btn.classList.remove("bg-green-700");
    }, 1500);
  }
})();
