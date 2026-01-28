<?php
class Obj {}
class A extends Obj {}
class B extends A {}
class C extends Obj {}
class D extends C {}

function takesD(D $d) : void {}

/** @param B|D $bar */
function foo(Obj $bar) : void {
    if (!$bar instanceof A) {
        takesD($bar);
    }
}