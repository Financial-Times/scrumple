'use strict';

const fs = require('fs');
const path = require('path');

var walk = function(dir) {
    var results = [];
    var list = fs.readdirSync(dir);
    list.forEach(function(file) {
        file = dir + '/' + file;
        var stat = fs.statSync(file);
        if (stat && stat.isDirectory()) { 
            /* Recurse into a subdirectory */
            results = results.concat(walk(file));
        } else { 
            /* Is a file */
            results.push(file);
        }
    });
    return results;
}

console.log('test', walk(__dirname));

const pathToOaxBinary = require('./index.js');
const oaxDestination = path.join(__dirname, './oax');

fs.unlinkSync(oaxDestination);
fs.symlinkSync(pathToOaxBinary, oaxDestination);
