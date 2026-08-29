/* ════════════════════════════════════════════════════════════
   ReUnite — Google Maps base styles

   Both are low-saturation on purpose, so the rose distress heatmap
   and the priority markers stay the only loud things on screen.
   ReUniteTheme decides which one ships.
   ════════════════════════════════════════════════════════════ */
window.ReUniteMapStyleLight = [
  { elementType: 'geometry',           stylers: [{ color: '#FBF7F4' }] },
  { elementType: 'labels.text.fill',   stylers: [{ color: '#6B5F62' }] },
  { elementType: 'labels.text.stroke', stylers: [{ color: '#FFFFFF' }, { weight: 3 }] },
  { elementType: 'labels.icon',        stylers: [{ visibility: 'off' }] },

  { featureType: 'administrative',             elementType: 'geometry.stroke',  stylers: [{ color: '#E6D8D2' }] },
  { featureType: 'administrative.land_parcel', stylers: [{ visibility: 'off' }] },
  { featureType: 'administrative.locality',    elementType: 'labels.text.fill', stylers: [{ color: '#4A403C' }] },

  { featureType: 'landscape.natural', elementType: 'geometry', stylers: [{ color: '#F6F2EE' }] },
  { featureType: 'poi',               elementType: 'labels',   stylers: [{ visibility: 'off' }] },
  { featureType: 'poi.park',          elementType: 'geometry', stylers: [{ color: '#E3EFE0' }] },

  { featureType: 'road',          elementType: 'geometry',        stylers: [{ color: '#FFFFFF' }] },
  { featureType: 'road',          elementType: 'geometry.stroke', stylers: [{ color: '#EFE3DC' }] },
  { featureType: 'road',          elementType: 'labels.text.fill',stylers: [{ color: '#8A7B76' }] },
  { featureType: 'road.highway',  elementType: 'geometry',        stylers: [{ color: '#FFE9C9' }] },
  { featureType: 'road.highway',  elementType: 'geometry.stroke', stylers: [{ color: '#F0CFA0' }] },
  { featureType: 'road.local',    elementType: 'labels',          stylers: [{ visibility: 'simplified' }] },

  { featureType: 'transit', stylers: [{ visibility: 'off' }] },
  { featureType: 'water',   elementType: 'geometry',         stylers: [{ color: '#D6E7F5' }] },
  { featureType: 'water',   elementType: 'labels.text.fill', stylers: [{ color: '#8FA9BF' }] }
];

window.ReUniteMapStyleDark = [
  { elementType: 'geometry',           stylers: [{ color: '#171B23' }] },
  { elementType: 'labels.text.fill',   stylers: [{ color: '#8A94A4' }] },
  { elementType: 'labels.text.stroke', stylers: [{ color: '#0F1115' }, { weight: 3 }] },
  { elementType: 'labels.icon',        stylers: [{ visibility: 'off' }] },

  { featureType: 'administrative',             elementType: 'geometry.stroke',  stylers: [{ color: '#2C333F' }] },
  { featureType: 'administrative.land_parcel', stylers: [{ visibility: 'off' }] },
  { featureType: 'administrative.locality',    elementType: 'labels.text.fill', stylers: [{ color: '#B6C0CE' }] },

  { featureType: 'landscape.natural', elementType: 'geometry', stylers: [{ color: '#151920' }] },
  { featureType: 'poi',               elementType: 'labels',   stylers: [{ visibility: 'off' }] },
  { featureType: 'poi.park',          elementType: 'geometry', stylers: [{ color: '#182219' }] },

  { featureType: 'road',         elementType: 'geometry',        stylers: [{ color: '#232935' }] },
  { featureType: 'road',         elementType: 'geometry.stroke', stylers: [{ color: '#1B2029' }] },
  { featureType: 'road',         elementType: 'labels.text.fill',stylers: [{ color: '#7A8494' }] },
  { featureType: 'road.highway', elementType: 'geometry',        stylers: [{ color: '#3A3226' }] },
  { featureType: 'road.highway', elementType: 'geometry.stroke', stylers: [{ color: '#4A3D2B' }] },
  { featureType: 'road.local',   elementType: 'labels',          stylers: [{ visibility: 'simplified' }] },

  { featureType: 'transit', stylers: [{ visibility: 'off' }] },
  { featureType: 'water',   elementType: 'geometry',         stylers: [{ color: '#0D1520' }] },
  { featureType: 'water',   elementType: 'labels.text.fill', stylers: [{ color: '#3F5A72' }] }
];

window.ReUniteMapStyle = window.ReUniteTheme
  ? window.ReUniteTheme.pick(window.ReUniteMapStyleLight, window.ReUniteMapStyleDark)
  : window.ReUniteMapStyleLight;
