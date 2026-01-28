<?php
/**
 * @psalm-return array{key1:int,key2:int}
 */
function foo(): array {
    $v = ["key1"=> 1, "key2"=> "2"];
    $r = array_map("intval", $v);
    return $r;
}
