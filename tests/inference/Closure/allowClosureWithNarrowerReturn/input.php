<?php
class A {}
class B extends A {}

/**
 * @param Closure():A $x
 */
function accept_closure($x) : void {
    $x();
}
accept_closure(
    function () : B {
        return new B();
    }
);
