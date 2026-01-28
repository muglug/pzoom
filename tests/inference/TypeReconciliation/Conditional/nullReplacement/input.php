<?php
/**
 * @param string|null|false $a
 * @return string|false $a
 */
function foo($a) {
  if ($a === null) {
    if (rand(0, 4) > 2) {
      $a = "hello";
    } else {
      $a = false;
    }
  }

  return $a;
}