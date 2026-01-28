<?php
class Obj {}
class A extends Obj {
    /** @var A|null */
    public $foo;
}
class B extends Obj {
    /** @var A|null */
    public $foo;
}
class C extends Obj {
    /** @var A|C|null */
    public $bar;
}

function takesA(A $a) : void {}

function foo(Obj $node) : void {
    while ($node instanceof A
        || $node instanceof B
        || ($node instanceof C && $node->bar instanceof A)
    ) {
        if (!$node instanceof C) {
            $node = $node->foo;
        } else {
            $node = $node->bar;
        }
    }
}
