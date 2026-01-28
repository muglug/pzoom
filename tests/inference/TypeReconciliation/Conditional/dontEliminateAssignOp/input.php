<?php
class Obj {}
class A extends Obj {}
class B extends A {}
class C extends Obj {}
class D extends C {}
class E extends C {}

function bar(Obj $node) : void {
    if ($node instanceof B
        || $node instanceof D
        || $node instanceof E
    ) {
        if ($node instanceof C) {}
        if ($node instanceof D) {}
    }
}