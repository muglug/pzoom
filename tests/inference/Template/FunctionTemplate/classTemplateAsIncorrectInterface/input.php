<?php
interface Foo {}
interface NotFoo {}

/**
 * @template T as Foo
 * @param T $x
 * @return T
 */
function bar($x) {
    return $x;
}

function takesNotFoo(NotFoo $f) : void {
    bar($f);
}
