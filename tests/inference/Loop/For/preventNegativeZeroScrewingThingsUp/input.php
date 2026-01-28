<?php
function foo() : void {
  /** @var array<int, int> $v */
  $v = [1 => 0];
  for ($d = 0; $d <= 10; $d++) {
    for ($k = -$d; $k <= $d; $k += 2) {
      if ($k === -$d || ($k !== $d && $v[$k-1] < $v[$k+1])) {
        $x = $v[$k+1];
      } else {
        $x = $v[$k-1] + 1;
      }

      $v[$k] = $x;
    }
  }
}
