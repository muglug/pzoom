<?php
/** @param string $str */
function foo($str): int {
  if (is_null($str)) {
    return 1;
  } else if (strlen($str) < 1) {
    return 2;
  }
  return 2;
}