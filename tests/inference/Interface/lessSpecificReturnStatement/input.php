<?php
interface A {}
interface B extends A {}

function foo(A $a): B {
    return $a;
}
