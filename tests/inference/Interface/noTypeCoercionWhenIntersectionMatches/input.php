<?php
interface I1 {}
interface I2 {}
class A implements I1 {}

/** @param A|I2 $i */
function foo($i) : void {}

/** @param I1&I2 $i */
function bar($i) : void {
    foo($i);
}
