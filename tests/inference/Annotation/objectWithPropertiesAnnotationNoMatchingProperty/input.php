<?php
/** @param object{foo:string} $o */
function foo(object $o) : string {
    return $o->foo;
}

class A {}

foo(new A);
