<?php
/**
 * @template TValue
 * @template TKey as array-key
 *
 * @param array<TKey, TValue> $arr
 */
function byRef(array &$arr) : void {}

$b = ["a" => 5, "c" => 6];
byRef($b);