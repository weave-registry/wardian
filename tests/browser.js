// Which browser the end-to-end tests launch. Locally they use Google Chrome, as they always have.
// CI (and anyone without Chrome) sets WARDIAN_BROWSER=chromium to use the Chromium that
// `npx playwright install chromium` downloads. Any other value is passed to Playwright as a channel
// (chrome-beta, msedge, ...).
const which = process.env.WARDIAN_BROWSER || 'chrome';
module.exports = (opts = {}) => (which === 'chromium' ? { ...opts } : { channel: which, ...opts });
