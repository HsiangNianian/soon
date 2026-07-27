const copyButtons = document.querySelectorAll("[data-copy]");

for (const button of copyButtons) {
  button.addEventListener("click", async () => {
    const originalLabel = button.textContent;

    try {
      await navigator.clipboard.writeText(button.dataset.copy);
      button.textContent = "Copied";
    } catch {
      button.textContent = "Select";
    }

    window.setTimeout(() => {
      button.textContent = originalLabel;
    }, 1600);
  });
}

const replayButton = document.querySelector("[data-replay]");
const demoImage = document.querySelector(".demo img");

replayButton?.addEventListener("click", () => {
  const source = demoImage.getAttribute("src").split("?")[0];
  demoImage.setAttribute("src", source + "?replay=" + Date.now());
});
