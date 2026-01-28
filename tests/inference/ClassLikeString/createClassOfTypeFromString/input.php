<?php
class A {}

/**
 * @return class-string<A> $s
 */
function foo(string $s) : string {
    if (!class_exists($s)) {
        throw new \UnexpectedValueException("bad");
    }

    if (!is_a($s, A::class, true)) {
        throw new \UnexpectedValueException("bad");
    }

    return $s;
}
