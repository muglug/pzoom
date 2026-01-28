<?php
/**
 * @param array<lowercase-string, bool> $foo
 * @return array<lowercase-string, bool>
 */
function foo(array $foo) : array {
    $foo["hello"] = true;
    return $foo;
}
