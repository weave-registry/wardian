const copy = document.querySelector('#copy');
copy.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText('curl -fsSL https://github.com/weave-registry/wardian/releases/latest/download/install.sh | sh\nwardian');
    copy.textContent = 'Copied ✓';
  } catch {
    copy.textContent = 'Select the commands above to copy';
  }
  setTimeout(() => { copy.textContent = 'Copy commands ⧉'; }, 3000);
});
