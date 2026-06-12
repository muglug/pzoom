<?php
class A1 {}
class A2 {}
class A3 {}

/** @psalm-assert-if-true A2|A3 $type */
function mayHave(object $type): bool {
    return $type instanceof A2 || $type instanceof A3;
}

function g(object $x): string {
    if (mayHave($x)) {
        return get_class($x);
    }
    return "";
}
