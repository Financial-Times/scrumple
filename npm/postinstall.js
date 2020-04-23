'use strict';

const fs = require('fs');
const path = require('path');

let pathToScrumpleBinary;
if (process.platform === "win32" && process.arch === "x64") {
    pathToScrumpleBinary = require('@financial-times/scrumple-windows-64');
} else if (process.platform === "darwin") {
    pathToScrumpleBinary = require('@financial-times/scrumple-darwin');
} else if (process.platform === "linux" && process.arch === "x64") {
    pathToScrumpleBinary = require('@financial-times/scrumple-linux-64');
} else {
    throw new Error('scrumple does not have a precompiled binary for the platform/architecture you are using. Please contact Origami or open an issue on https://github.com/Financial-Times/scrumple/issues');
}

const scrumpleDestination = './scrumple';

fs.unlinkSync(scrumpleDestination);
fs.symlinkSync(path.relative(__dirname, pathToScrumpleBinary), scrumpleDestination);
