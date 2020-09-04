import odd from './odd.js'

export default function even(n) {
  return n == 0 || odd(n - 1)
}
