const copy = document.querySelector('#copy');
copy.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText('cargo install --path .\nwardian');
    copy.textContent = 'Copied ✓';
  } catch {
    copy.textContent = 'Select the commands above to copy';
  }
  setTimeout(() => { copy.textContent = 'Copy commands ⧉'; }, 3000);
});
