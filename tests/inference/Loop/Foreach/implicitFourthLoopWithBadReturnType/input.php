<?php
function test(): int {
  $x = 0;
  $y = 1;
  $z = 2;
  foreach ([0, 1, 2] as $i) {
    $x = $y;
    $y = $z;
    $z = "hello";
  }
  return $x;
}
