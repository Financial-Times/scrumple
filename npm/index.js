'use strict';

module.exports = (function(){
  try {
    return require('@financial-times/scrumple-linux-64');
  } catch {
    try {
      return require('@financial-times/scrumple-darwin');
    } catch {
      try {
        return require('@financial-times/scrumple-windows-64');
      } catch {
        throw new Error('scrumple does not have a precompiled binary for the platform/architecture you are using. Please contact Origami or open an issue on https://github.com/Financial-Times/scrumple/issues');
      }
    }
  }
}());
