<?php
/**
 * @template T as object
 * @param T $o
 * @return T
 */
function takesObject(object $o) : object {
    return $o;
}

class A {}

/** @psalm-suppress PossiblyNullArgument */
$a = takesObject(rand(0, 1) ? new A() : null);