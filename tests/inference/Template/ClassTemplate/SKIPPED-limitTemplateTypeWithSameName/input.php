<?php
/**
 * @template T as object
 */
abstract class A {}

function takesA(A $a) : void {}

/** @param A<stdClass> $a */
function foo(A $a) : void {
    takesA($a);
}
