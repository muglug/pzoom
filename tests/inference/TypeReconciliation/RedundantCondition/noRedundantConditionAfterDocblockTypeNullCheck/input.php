<?php
class A {
    /** @var ?int */
    public $foo;
}
class B {}

/**
 * @param  A|B $i
 */
function foo($i): void {
    if (empty($i)) {
        return;
    }

    switch (get_class($i)) {
        case A::class:
            if ($i->foo !== null) {}
            break;

        default:
            break;
    }
}
