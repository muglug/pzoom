<?php
/**
 * @psalm-param -1|0|1 $i
 */
 function onlyZeroOrPlusMinusOne(int $i): int {
     return $i;
 }

 /**
  * @psalm-param mixed $a
  * @psalm-param mixed $b
  */
 function foo($a, $b): void {
     onlyZeroOrPlusMinusOne($a <=> $b);
 }
