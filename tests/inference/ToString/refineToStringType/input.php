<?php
/** @psalm-return non-empty-string */
function doesCast() : string {
    return (string) (new A());
}

/** @psalm-return non-empty-string */
function callsToString() : string {
    return (new A())->__toString();
}

class A {
    /** @psalm-return non-empty-string */
    function __toString(): string {
        return "ha";
    }
}
