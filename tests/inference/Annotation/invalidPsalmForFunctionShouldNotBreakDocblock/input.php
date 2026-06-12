<?php
/**
 * @psalm-impure
 * @param string $arg
 * @return non-falsy-string
 */
function foo($arg) {
    return $arg . "bar";
}

$_ = foo("hello");
