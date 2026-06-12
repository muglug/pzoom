<?php
class C implements Stringable { public function __toString(): string { return __CLASS__; } }

/** @return stringable-object */
function f(): object {
    return new C;
}
