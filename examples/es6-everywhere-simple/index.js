import itt from 'itt'
import { square, cube } from './math.js'

console.log(itt.range(10).map(square).join(' '))
console.log(itt.range(10).map(cube).join(' '))
