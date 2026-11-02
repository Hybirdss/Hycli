import assert from 'node:assert/strict';

// selectOption only dispatches the UI event. A suite must not close its context
// until the shared fixture has committed the setting for the next suite.
export async function selectPersistedLocale(page, select, locale) {
  if (await select.inputValue() !== locale) {
    const saved = page.waitForResponse(response => {
      const request = response.request();
      return new URL(response.url()).pathname === '/api/settings'
        && request.method() === 'PATCH' && request.postDataJSON()?.locale === locale;
    });
    await select.selectOption(locale);
    assert.equal((await saved).status(), 200, 'Locale update must succeed');
  }
  await page.waitForFunction(async expected => {
    const response = await fetch('/api/state');
    return response.ok && (await response.json()).settings.locale === expected;
  }, locale);
}
