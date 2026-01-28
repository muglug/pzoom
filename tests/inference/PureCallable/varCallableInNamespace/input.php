<?php
namespace Foo;

/**
 * @param pure-callable $c
 */
function bar(callable $c) : callable {
    return $c;
}
