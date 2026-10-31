// Every section and language remains available without JavaScript.
document.querySelector('.time-controls').hidden = false;
const preview = document.querySelector('#pixel-preview');
const timeButtons = document.querySelectorAll('[data-time]');
timeButtons.forEach(button => {
  button.addEventListener('click', () => {
    preview.src = `screenshots/pixoo64-miniature-${button.dataset.time}.png`;
    preview.alt = button.dataset.alt;
    timeButtons.forEach(item => item.setAttribute('aria-pressed', String(item === button)));
    document.querySelector('#preview-caption').textContent = button.dataset.caption;
  });
});
const animationButton = document.querySelector('#animation-toggle');
const board = document.querySelector('#departure-board');
if (animationButton && board) {
  animationButton.hidden = false;
  animationButton.addEventListener('click', () => {
    const playing = animationButton.getAttribute('aria-pressed') !== 'true';
    board.src = playing ? 'screenshots/board-desktop.gif' : 'assets/departure-board.webp';
    animationButton.setAttribute('aria-pressed', String(playing));
    animationButton.textContent = playing ? animationButton.dataset.pause : animationButton.dataset.play;
  });
  // Stop a user-started animation when the system preference changes.
  matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', event => {
    if (event.matches && animationButton.getAttribute('aria-pressed') === 'true') animationButton.click();
  });
}
