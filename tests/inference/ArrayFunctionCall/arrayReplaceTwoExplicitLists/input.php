<?php
/**
 * @param list<int> $foo
 */
function foo(array $foo) : void {}

$foo1 = [1, 2, 3];
$foo2 = [1, 4, 5];
foo(array_replace($foo1, $foo2));
