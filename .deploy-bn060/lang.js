/* Basic Next landing language by geolocation + explicit preference */
(function () {
  var KEY = 'basicnext-lang';
  var path = location.pathname.replace(/\/+$/, '') || '/';
  var onEn = path === '/basicnext/en' || path.indexOf('/basicnext/en/') === 0;
  var onPt = path === '/basicnext' || path === '/basicnext/';
  if (!onEn && !onPt) return;

  var pref = localStorage.getItem(KEY);
  if (pref === 'pt' && onEn) { location.replace('/basicnext/'); return; }
  if (pref === 'en' && onPt) { location.replace('/basicnext/en/'); return; }
  if (pref === 'pt' || pref === 'en') return;

  fetch('https://get.geojs.io/v1/ip/country.json', { credentials: 'omit' })
    .then(function (r) { return r.ok ? r.json() : Promise.reject(); })
    .then(function (d) {
      var cc = String((d && d.country) || '').toUpperCase();
      var lusophone = cc === 'BR' || cc === 'PT' || cc === 'MZ';
      if (lusophone && onEn) location.replace('/basicnext/');
      else if (!lusophone && onPt) location.replace('/basicnext/en/');
    })
    .catch(function () {});
})();
