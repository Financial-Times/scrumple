'use strict';

module.exports = (function(){
  try {
    return require('oax-linux-64');
  } catch {
    try { 
      return require('oax-darwin');
    } catch {
      try {
        return require('oax-windows-64');
      } catch {
        throw new Error('oax does not have a precompiled binary for the platform/architecture you are using. Please contact Origami or open an issue on https://github.com/Financial-Times/oax/issues');
      }
    }
  }
}());
