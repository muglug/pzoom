<?php
/**
 * @template TValue
 *
 * @param array<TValue> $arr
 */
function byRef(array &$arr) : void {}

$b = ["a" => 5, "c" => 6];
byRef($b);