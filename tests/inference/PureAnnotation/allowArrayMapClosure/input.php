<?php
/**
 * @psalm-pure
 * @param string[] $arr
 */
function foo(array $arr) : array {
    return \array_map(function(string $s) { return $s;}, $arr);
}
