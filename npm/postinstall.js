'use strict';

const fs = require('fs');
const path = require('path');

const pathToOaxBinary = require('./index.js');
const oaxDestiation = path.join(__dirname, './oax');

fs.copyFileSync(pathToOaxBinary, oaxDestiation);