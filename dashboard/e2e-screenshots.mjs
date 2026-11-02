// Aggregate verification must not regenerate the reviewed documentation assets.
// Pass a function when screenshot-only preparation (image decoding or cropping)
// should also be skipped. Set HYCLI_NO_SCREENSHOTS=0 to regenerate deliberately.
export async function documentationScreenshot(page, options) {
  if (process.env.HYCLI_NO_SCREENSHOTS === '1') return;
  const resolvedOptions = typeof options === 'function' ? await options() : options;
  return page.screenshot({ animations: 'disabled', ...resolvedOptions });
}
