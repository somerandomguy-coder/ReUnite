const fs = require('fs');
const path = require('path');

// Look for key in environment variables (support GOOGLE_MAPS_API_KEY or YOUR_GOOGLE_MAPS_API_KEY)
const apiKey = process.env.GOOGLE_MAPS_API_KEY || process.env.YOUR_GOOGLE_MAPS_API_KEY || '';

const examplePath = path.join(__dirname, 'assets', 'js', 'config.example.js');
const targetPath = path.join(__dirname, 'assets', 'js', 'config.js');

try {
  let content = fs.readFileSync(examplePath, 'utf8');

  if (apiKey) {
    // Replace placeholder with environment variable value
    content = content.replace('YOUR_GOOGLE_MAPS_API_KEY', () => apiKey);
    console.log('[ReUnite build] Successfully injected API key from environment variable into assets/js/config.js');
  } else {
    console.warn('[ReUnite build] Warning: Neither GOOGLE_MAPS_API_KEY nor YOUR_GOOGLE_MAPS_API_KEY was found in environment. Generated config.js with placeholder.');
  }

  fs.writeFileSync(targetPath, content, 'utf8');
} catch (err) {
  console.error('[ReUnite build] Error building config.js:', err);
  process.exit(1);
}
