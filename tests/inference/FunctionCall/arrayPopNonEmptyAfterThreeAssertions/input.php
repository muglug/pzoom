<?php
class A {}
class B extends A {
    /** @var array<int, string> */
    public $arr = [];
}

/** @var array<A> */
$replacement_stmts = [];

if (!$replacement_stmts
    || !$replacement_stmts[0] instanceof B
    || count($replacement_stmts[0]->arr) > 1
) {
    return null;
}

$b = $replacement_stmts[0]->arr;
