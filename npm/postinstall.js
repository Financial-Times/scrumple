'use strict';

const fs = require('fs');
const path = require('path');

const pathToOaxBinary = require('./index.js');
const oaxDestination = path.join(__dirname, './oax');

fs.unlinkSync(oaxDestination);
fs.symlinkSync(pathToOaxBinary, oaxDestination);
