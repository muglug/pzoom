<?php
class A {}

/**
 * @return class-string<A> $s
 */
function foo(string $s) : string {
    if (!class_exists($s)) {
        throw new \UnexpectedValueException("bad");
    }

    if (!is_subclass_of($s, A::class)) {
        throw new \UnexpectedValueException("bad");
    }

    return $s;
}
