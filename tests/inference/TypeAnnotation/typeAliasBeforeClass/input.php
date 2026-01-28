<?php
namespace Barrr;

/**
 * @psalm-type CoolType = A|B|null
 */

class A {}
class B {}

/** @return CoolType */
function foo() {
    if (rand(0, 1)) {
        return new A();
    }

    if (rand(0, 1)) {
        return new B();
    }

    return null;
}

/** @param CoolType $a **/
function bar ($a) : void { }

bar(foo());
