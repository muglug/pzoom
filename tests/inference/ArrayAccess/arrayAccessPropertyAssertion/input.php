<?php
class A {}
class B extends A {
    /** @var array<int, string> */
    public $arr = [];
}

/** @var array<A> */
$as = [];

if (!$as
    || !$as[0] instanceof B
    || !$as[0]->arr
) {
    return null;
}

$b = $as[0]->arr;
