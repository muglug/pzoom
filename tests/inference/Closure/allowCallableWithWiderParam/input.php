<?php
class A {}
class B extends A {}

/**
 * @param Closure(B $a):A $x
 */
function accept_closure($x) : void {
    $x(new B());
}
accept_closure(
    function (A $a) : A {
        return $a;
    }
);
