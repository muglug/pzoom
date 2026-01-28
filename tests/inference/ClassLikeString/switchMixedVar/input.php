<?php
class A {}
class B {}
class C {}

/** @param mixed $a */
function foo($a) : void {
    switch ($a) {
        case A::class:
            return;

        case B::class:
        case C::class:
            return;
    }
}
