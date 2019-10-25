# oax 

> Prebuilt oax binaries available via npm

## API

```
$ npm install --save oax
```

```js
const execFile = require('child_process').execFile;
const oax = require('oax');

execFile(oax, ['index.js'], (err, stdout) => {
	console.log(stdout);
});
```
