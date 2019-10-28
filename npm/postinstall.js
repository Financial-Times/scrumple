'use strict';

const fs = require('fs');
const path = require('path');

let pathToOaxBinary;
if (process.platform === "win32" && process.arch === "x64") {
    pathToOaxBinary = require('oax-windows-64');
} else if (process.platform === "darwin") {
    pathToOaxBinary = require('oax-darwin');
} else if (process.platform === "linux" && process.arch === "x64") {
    pathToOaxBinary = require('oax-linux-64');
} else {
    throw new Error('oax does not have a precompiled binary for the platform/architecture you are using. Please contact Origami or open an issue on https://github.com/Financial-Times/oax/issues');
}

const oaxDestination = path.join(__dirname, './oax');

fs.unlinkSync(oaxDestination);
fs.symlinkSync(pathToOaxBinary, oaxDestination);
