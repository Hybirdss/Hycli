// Shared by the popup and background worker. Browser permission scope is NOT
// execution scope: adding a parent grant never changes the paired site origin.
globalThis.HYCLI_SITE_SCOPE = (() => {
  function plan(value, firefox = false) {
    const site = new URL(value);
    if (!['https:', 'http:'].includes(site.protocol) || site.username || site.password) throw new Error('Invalid website');
    const parsed = tldts.parse(site.hostname, {allowPrivateDomains:true});
    // Unknown suffixes and local names have no trustworthy registrable parent.
    const parent = (parsed.isIcann || parsed.isPrivate) && !parsed.isIp && !['home.arpa','onion'].includes(parsed.publicSuffix) ? parsed.domain : null;
    const origins = [site.origin];
    if (parent && parent !== site.hostname && !site.hostname.endsWith('.')) {
      let ancestor = site.hostname;
      while (ancestor !== parent) {
        ancestor = ancestor.slice(ancestor.indexOf('.') + 1);
        const related = new URL(site.origin); related.hostname = ancestor;
        origins.push(related.origin);
      }
    }
    return {origin:site.origin, hostname:site.hostname, parent, origins,
      // Cookies are host-oriented, not port-oriented; Firefox also rejects
      // ports in match patterns. Display the actual host grant, but retain
      // the exact port in origin for observation and execution boundaries.
      permissions:origins.map(origin => { const url=new URL(origin); url.port=''; return url.origin+'/*'; }),
      topLevelSite:site.protocol + '//' + (parent || site.hostname)};
  }
  function allowsCookie(cookie, scope, storeId) {
    if (cookie.storeId !== storeId) return false;
    const domain = cookie.domain.replace(/^\./, '').toLowerCase();
    if (scope.hostname !== domain && (cookie.hostOnly !== false || !scope.hostname.endsWith('.' + domain))) return false;
    // Public suffix cookies must not cross tenant boundaries, even in synthetic
    // or malformed input. Host-only cookies can only match their exact host.
    if (!cookie.hostOnly && domain !== scope.hostname && (!scope.parent || plan('https://' + domain).parent !== scope.parent)) return false;
    if (cookie.secure && !scope.origin.startsWith('https:')) return false;
    if (!cookie.path?.startsWith('/')) return false;
    if (cookie.expirationDate && cookie.expirationDate <= Date.now()/1000) return false;
    if (cookie.firstPartyDomain && cookie.firstPartyDomain !== (scope.parent || scope.hostname)) return false;
    if (cookie.partitionKey) {
      if (cookie.partitionKey.hasCrossSiteAncestor) return false;
      try { if (plan(cookie.partitionKey.topLevelSite).topLevelSite !== scope.topLevelSite) return false; }
      catch { return false; }
    }
    return true;
  }
  async function cookies(api, scope, storeId, firefox, permissions = scope.permissions) {
    if (!await api.permissions.contains({origins:permissions})) throw new Error('Website access is unavailable');
    const available = await api.cookies.getAll({storeId,...(firefox?{firstPartyDomain:null}:{})});
    // Old engines may not implement partition queries. Never interpret failure
    // of the primary query as an empty signed-out session.
    if (!await api.permissions.contains({origins:permissions})) throw new Error('Website access is unavailable');
    try { available.push(...await api.cookies.getAll({url:scope.origin,storeId,...(firefox?{firstPartyDomain:null}:{}),partitionKey:{topLevelSite:scope.topLevelSite}})); } catch {}
    const result = new Map();
    for (const cookie of available) if (allowsCookie(cookie,scope,storeId)) {
      result.set(JSON.stringify([cookie.name,cookie.domain,cookie.path,cookie.storeId,cookie.firstPartyDomain || '',cookie.partitionKey || null]),cookie);
    }
    return [...result.values()];
  }
  return {plan,allowsCookie,cookies};
})();
