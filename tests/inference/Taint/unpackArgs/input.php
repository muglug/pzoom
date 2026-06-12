<?php
function test(...$args) {
    echo $args[0];
}

/**
 * @psalm-taint-source input
 */
function getQueryParam() {}

// cannot use $_GET, see #8477
$foo = getQueryParam();
test(...$foo);
