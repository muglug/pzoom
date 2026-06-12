<?php
interface I {}

class A implements I {
    public function bar() : void {}
}

/** @return I[] */
function foo() : array {
    return [new A()];
}

/** @var A $a1 */
[$a1, $_a2] = foo();

$a1->bar();
