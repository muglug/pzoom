<?php
class C implements Stringable { public function __toString(): string { return __CLASS__; } }

/** @param stringable-object $p */
function f(object $p): Stringable {
    return $p;
}
