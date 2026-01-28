<?php
/**
 * @template T as object
 */
abstract class A {}

function takesA(A $a) : void {}

function foo(A $a) : void {
    takesA($a);
}