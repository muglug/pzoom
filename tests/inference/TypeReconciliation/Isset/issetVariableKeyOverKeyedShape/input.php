<?php
/** @param list<string> $_values */
function takesStringList(array $_values): void {}

function f(string $key, int $offset): void {
    $map = ['a' => [['html']]];
    if (isset($map[$key][$offset])) {
        takesStringList($map[$key][$offset]);
    }
}
