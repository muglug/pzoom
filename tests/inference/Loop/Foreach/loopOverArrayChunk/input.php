<?php
/**
* @return array<int, array<array-key, int>>
*/
function Foo(int $a, int $b, int ...$ints) : array {
  array_unshift($ints, $a, $b);

  return array_chunk($ints, 2);
}

foreach(Foo(1, 2, 3, 4, 5) as $ints) {
  echo $ints[0], ", ", ($ints[1] ?? "n/a"), "\n";
}
