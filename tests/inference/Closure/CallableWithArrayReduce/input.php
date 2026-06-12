<?php
/**
 * @return callable(int, int): int
 */
function maker() {
   return function(int $sum, int $e) {
      return $sum + $e;
   };
}
$maker = maker();
$result = array_reduce([1, 2, 3], $maker, 0);
