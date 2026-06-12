<?php
/**
 * @psalm-suppress UndefinedClass
 */
function fooFoo(): A {
    return $GLOBALS["a"];
}

fooFoo()->bar();
