<?php
class A {}
class B extends A {}
class C extends B {}

function bad(A $x) : void {
    if (($x instanceof C && rand(0, 1)) || rand(0, 1)) {
        return;
    }

    if ($x instanceof B) {
        if ($x instanceof C) {}
    }
}