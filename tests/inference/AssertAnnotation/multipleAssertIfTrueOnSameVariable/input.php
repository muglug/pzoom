<?php
class A {}

function foo(string|null|A $a) : A {
    if (isComputed($a)) {
        return $a;
    }

    throw new Exception("bad");
}

/**
 * @psalm-assert-if-true !null $value
 * @psalm-assert-if-true !string $value
 */
function isComputed(mixed $value): bool {
    return $value !== null && !is_string($value);
}
