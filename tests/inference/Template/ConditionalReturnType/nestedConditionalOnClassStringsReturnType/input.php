<?php
class A {}
class B {}

/**
 * @template T as string
 * @param T $i
 * @psalm-return (T is A::class ? string : (T is B::class ? int : bool))
 */
function getDifferentType(string $i) {
    if ($i === A::class) {
        return "hello";
    }

    if ($i === B::class) {
        return 5;
    }

    return true;
}