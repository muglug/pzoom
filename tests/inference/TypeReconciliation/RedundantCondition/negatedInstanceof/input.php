<?php
class A {}
class B {}

function foo(A $a) : void {
    if (!$a instanceof B) {}
}
