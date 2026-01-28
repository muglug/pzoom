<?php
/**
 * @template T as object
 * @param T|class-string<T> $s
 * @return T
 * @psalm-suppress MixedMethodCall
 */
function bar($s) {
    if (is_object($s)) {
        return $s;
    }

    return new $s();
}

function foo(string $s) : object {
    /** @psalm-suppress ArgumentTypeCoercion */
    return bar($s);
}