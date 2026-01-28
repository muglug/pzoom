<?php
class A {
    public function foo() : void {}
}

function takesA(A $a) : bool {
    return true;
}

/**
 * @param mixed $a
 */
function takesMaybeA($a) : void {
    /**
     * @psalm-suppress MixedArgument
     */
    if ($a !== null && takesA($a)) {}
}