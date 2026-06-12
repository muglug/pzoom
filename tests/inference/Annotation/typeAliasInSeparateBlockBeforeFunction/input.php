<?php
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

/** @param CoolType $_a **/
function bar ($_a) : void { }

bar(foo());
