<?php
/**
 * @psalm-return array{key1:int,key2:int}
 */
function foo(): array {
  $v = ["key1"=> 1, "key2"=> "2"];
  $r = array_map(function($i) : int { return intval($i);}, $v);
  return $r;
}
