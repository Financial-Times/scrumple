const alternateMain = require('alternate-main');
const alternateFiles = require('alternate-files/foo');
const alternateFilesNested = require('alternate-files/nested/foo');
const alternateFilesInternal = require('alternate-files/internal-import');
const alternateModule = require('alternate-module');
const ignored = require('ignored');

console.log('In browser should be true and false otherwise');
console.log('alternateMain:', alternateMain.isBrowser);
console.log('alternateFiles:', alternateFiles.isBrowser);
console.log('alternateFilesNested:', alternateFilesNested.isBrowser);
console.log('alternateFilesInternal:', alternateFilesInternal.isBrowser);
console.log('alternateModule:', alternateModule.isBrowser);
console.log('ignored:', ignored.isBrowser);
