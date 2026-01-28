<?php
namespace Barrr;

/**
 * @psalm-type CoolType = A|B|null
 */
/**
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
