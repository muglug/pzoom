<?php
class C {
    /** @var string|null */
    public $foo;
}

/** @param mixed $a */
function barBar(C $c, $a): void
{
    $c->foo = $a;
}
