const { withAndroidManifest, withAndroidResources } = require('expo/config-plugins');
const path = require('path');
const fs = require('fs');

module.exports = function withCleartextTraffic(config) {
  config = withAndroidManifest(config, (config) => {
    const mainApplication = config.modResults.manifest.application[0];
    mainApplication.$['android:usesCleartextTraffic'] = 'true';
    return config;
  });

  config = withAndroidResources(config, (config) => {
    const xmlDir = path.join(config.modRequest.platformProjectRoot, 'app/src/main/res/xml');
    if (!fs.existsSync(xmlDir)) {
      fs.mkdirSync(xmlDir, { recursive: true });
    }
    const src = path.join(config.modRequest.projectRoot, 'network-security-config.xml');
    const dst = path.join(xmlDir, 'network_security_config.xml');
    if (fs.existsSync(src) && !fs.existsSync(dst)) {
      fs.copyFileSync(src, dst);
    }
    return config;
  });

  return config;
};
