<?php
class A {}
class B {}
class C {}

/**
 * @param Closure(B):A $f
 * @param Closure(C):B $g
 *
 * @return Closure(C):A
 */
function foo(Closure $f, Closure $g) : Closure {
    return function (C $x) use ($f, $g) : A {
        return $f($g($x));
    };
}

$a = foo(
    function(B $b) : A { return new A;},
    function(C $c) : B { return new B;}
)(new C);
