<?php
class Obj {}
class A extends Obj {
    /** @var A|null */
    public $foo;
}
class B extends Obj {}

function foo(Obj $node) : void {
    while ($node instanceof A
        || $node instanceof B
    ) {
        if (!$node instanceof B) {
            $node = $node->foo;
        }
    }
}
