<?php
/** @return Generator<int, string, int, int> */
function gen(): Generator {
    yield 3 => "abc";

    $foo = 4;

    return $foo;
}
