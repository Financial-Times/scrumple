# scrumple

> Prebuilt scrumple binaries available via npm

## API

```
$ npm install --save scrumple
```

```js
const execFile = require('child_process').execFile;
const scrumple = require('scrumple');

execFile(scrumple, ['index.js'], (err, stdout) => {
	console.log(stdout);
});
```
