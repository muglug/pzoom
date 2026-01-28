<?php

/**
 * @template T
 * @param T $a
 * @param T $b
 * @return T
 */
function foo($a, $b) {
  return rand(0, 1) ? $a : $b;
}

echo foo([], "hello");
