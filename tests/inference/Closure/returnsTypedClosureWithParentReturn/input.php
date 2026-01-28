<?php
class A {}
class B {}
class C {}
class A2 extends A {}

/**
 * @param Closure(B):A2 $f
 * @param Closure(C):B $g
 *
 * @return Closure(C):A
 */
function foo(Closure $f, Closure $g) : Closure {
    return function (C $x) use ($f, $g) : A2 {
        return $f($g($x));
    };
}

$a = foo(
    function(B $b) : A2 { return new A2;},
    function(C $c) : B { return new B;}
)(new C);
