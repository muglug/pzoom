<?php
/**
 * @psalm-type CoolType = A|B|null
 */

// this breaks up the line

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

/** @param CoolType $_a **/
function bar ($_a) : void { }

bar(foo());
