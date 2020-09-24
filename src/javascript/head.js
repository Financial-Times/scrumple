(function() {
  var Scrumple = {}
  Scrumple.baseRequire = typeof require !== "undefined" ? require : function(n) {
    throw new Error("Could not resolve module name: " + n)
  }
  Scrumple.ignored = function(){}
  Scrumple.ignored.deps = {}
  Scrumple.ignored.filename = ''
  Scrumple.modules = {}
  Scrumple.files = {}
  Scrumple.mains = {}
  Scrumple.resolve = function (base, then) {
    base = base.split('/')
    base.shift()
    then.split('/').forEach(function(p) {
      if (p === '..') base.pop()
      else if (p !== '.') base.push(p)
    })
    return '/' + base.join('/')
  }
  Scrumple.Module = function Module(filename, parent) {
    this.filename = filename
    this.id = filename
    this.loaded = false
    this.parent = parent
    this.children = []
    this.exports = {}
  }
  Scrumple.makeRequire = function (self) {
    var require = function(m) { return require._module(m).exports }
    require._deps = {}
    require.main = self

    require._esModule = function (m) {
      var mod = require._module(m)
      return mod.exports.__esModule ? mod.exports : {
        get default() {return mod.exports}
      }
    }
    require._module = function (m) {
      var fn = self ? require._deps[m] : Scrumple.main
      if (fn == null) {
        var module = {exports: Scrumple.baseRequire(m)}
        require._deps[m] = {module: module}
        return module
      }
      if (fn.module) return fn.module
      var module = new Scrumple.Module(fn.filename, self)
      fn.module = module
      module.require = Scrumple.makeRequire(module)
      module.require._deps = fn.deps
      module.require.main = self ? self.require.main : module
      if (self) self.children.push(module)
      fn(module, module.exports, module.require, fn.filename, fn.filename.split('/').slice(0, -1).join('/'), {url: 'file://' + (fn.filename.charAt(0) === '/' ? '' : '/') + fn.filename})
      module.loaded = true
      return module
    }
    return require
  }
