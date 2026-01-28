<?php
/**
 * @param array<string, int> $a
 * @return array<string, int>
 */
function foo($a)
{
  return $a;
}

$a1 = ["hi" => 3];
$a2 = ["bye" => 5];
$a3 = array_merge($a1, $a2);

foo($a3);
