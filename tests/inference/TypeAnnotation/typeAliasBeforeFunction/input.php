<?php
namespace Barrr;

/**
 * @psalm-type A_OR_B = A|B
 * @psalm-type CoolType = A_OR_B|null
 * @return CoolType
 */
function foo() {
    if (rand(0, 1)) {
        return new A();
    }

    if (rand(0, 1)) {
        return new B();
    }

    return null;
}

class A {}
class B {}

/** @param CoolType $a **/
function bar ($a) : void { }

bar(foo());
