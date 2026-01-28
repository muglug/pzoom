<?php
/**
 * @param non-empty-list<string>|array{null} $arr
 * @return array<int, string>
 */
function foo(array $arr) {
    array_shift($arr);
    return $arr;
}
